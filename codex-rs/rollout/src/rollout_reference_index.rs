//! Indexes direct fork references found in local rollout files.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use codex_protocol::RolloutId;
use codex_protocol::ThreadId;
use codex_protocol::protocol::HistoryPosition;

use crate::ARCHIVED_SESSIONS_SUBDIR;
use crate::SESSIONS_SUBDIR;
use crate::compression::RolloutFile;

/// Direct history-base edges discovered from local rollout metadata.
///
/// This indexes immutable rollout IDs, not a thread's selected lineage. Callers use it to answer
/// cheap inverse-reference questions without each reimplementing rollout discovery.
#[derive(Debug, Default)]
pub struct RolloutReferenceIndex {
    rollouts_by_id: HashMap<RolloutId, IndexedRollout>,
    reference_counts_by_rollout: HashMap<RolloutId, usize>,
}

#[derive(Debug)]
struct IndexedRollout {
    thread_id: ThreadId,
    path: PathBuf,
    history_base: Option<HistoryPosition>,
}

impl RolloutReferenceIndex {
    /// Scans active and archived local rollout metadata.
    pub async fn scan(codex_home: &Path) -> io::Result<Self> {
        Self::scan_paths(vec![
            codex_home.join(ARCHIVED_SESSIONS_SUBDIR),
            codex_home.join(SESSIONS_SUBDIR),
        ])
        .await
    }

    /// Scans only unarchived rollouts to locate files that still need to be archived.
    ///
    /// Reference counts exclude archived history and must not be used to decide whether a
    /// rollout can be deleted or compressed.
    pub async fn scan_unarchived(codex_home: &Path) -> io::Result<Self> {
        Self::scan_paths(vec![codex_home.join(SESSIONS_SUBDIR)]).await
    }

    async fn scan_paths(mut stack: Vec<PathBuf>) -> io::Result<Self> {
        let mut rollouts_by_id = HashMap::new();
        while let Some(directory) = stack.pop() {
            let mut entries = match tokio::fs::read_dir(directory.as_path()).await {
                Ok(entries) => entries,
                Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
                Err(err) => return Err(err),
            };
            loop {
                let Some(entry) = entries.next_entry().await? else {
                    break;
                };
                let path = entry.path();
                let file_type = entry.file_type().await?;
                if file_type.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }
                let Some(rollout_file) = RolloutFile::from_path(path) else {
                    continue;
                };
                let Some(rollout_id) = crate::rollout_id_from_path(rollout_file.path()) else {
                    continue;
                };
                let Ok(meta) = crate::read_session_meta_line(rollout_file.path()).await else {
                    continue;
                };
                if let Entry::Vacant(entry) = rollouts_by_id.entry(rollout_id) {
                    entry.insert(IndexedRollout {
                        thread_id: meta.meta.id,
                        path: rollout_file.into_path(),
                        history_base: meta.meta.history_base,
                    });
                }
            }
        }

        let mut reference_counts_by_rollout = HashMap::new();
        for (rollout_id, rollout) in &rollouts_by_id {
            let Some(history_base) = rollout.history_base else {
                continue;
            };
            if history_base.thread_id == *rollout_id {
                continue;
            }
            *reference_counts_by_rollout
                .entry(history_base.thread_id)
                .or_default() += 1;
        }
        Ok(Self {
            rollouts_by_id,
            reference_counts_by_rollout,
        })
    }

    /// Returns how many other discovered rollouts directly reference `rollout_id`.
    pub fn reference_count(&self, rollout_id: RolloutId) -> usize {
        self.reference_counts_by_rollout
            .get(&rollout_id)
            .copied()
            .unwrap_or_default()
    }

    /// Returns the direct history-base edge for `rollout_id`, if one was discovered.
    pub fn history_base(&self, rollout_id: RolloutId) -> Option<&HistoryPosition> {
        self.rollouts_by_id
            .get(&rollout_id)
            .and_then(|rollout| rollout.history_base.as_ref())
    }

    /// Returns rollout IDs and paths whose session metadata belongs to `thread_id`.
    pub fn rollouts_for_thread(
        &self,
        thread_id: ThreadId,
    ) -> impl Iterator<Item = (RolloutId, &Path)> {
        self.rollouts_by_id
            .iter()
            .filter(move |(_, rollout)| rollout.thread_id == thread_id)
            .map(|(rollout_id, rollout)| (*rollout_id, rollout.path.as_path()))
    }
}

#[cfg(test)]
#[path = "rollout_reference_index_tests.rs"]
mod tests;

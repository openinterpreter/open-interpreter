//! Captures bounded, canonical paths for invoked user-owned skills.

use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use codex_core::config::Config;
use codex_extension_api::ContentItemKind;
use codex_extension_api::ContextualUserFragment;
use codex_protocol::protocol::TruncationPolicy;
use dirs::home_dir;
use serde_json::json;

const MAX_TRUSTED_SKILLS: usize = 16;
const MAX_TRUSTED_SKILL_PATH_BYTES: usize = 512;
const MAX_TRUSTED_SKILL_PATHS_BYTES: usize = 2_048;
const MAX_TRUSTED_SKILL_TOKENS: usize = 768;
const TRUSTED_SKILLS_PREFIX: &str = "Codex-verified invoked user-owned skill paths:\n";

/// Host-verified user-owned skill paths for the current Guardian classification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GuardianTrustedSkillsFragment {
    pub(crate) paths: Vec<String>,
}

impl ContextualUserFragment for GuardianTrustedSkillsFragment {
    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind("guardian.trusted_skills".to_owned())
    }

    fn role(&self) -> &'static str {
        "developer"
    }

    fn requires_separate_message(&self) -> bool {
        true
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn type_markers() -> (&'static str, &'static str) {
        ("", "")
    }

    fn body(&self) -> String {
        let mut paths = String::from("[");
        let max_path_bytes = TruncationPolicy::Tokens(MAX_TRUSTED_SKILL_TOKENS)
            .byte_budget()
            .saturating_sub(TRUSTED_SKILLS_PREFIX.len());
        for path in &self.paths {
            let separator = if paths.ends_with('[') { "" } else { "," };
            let encoded_path = json!(path).to_string();
            if paths
                .len()
                .saturating_add(separator.len())
                .saturating_add(encoded_path.len())
                .saturating_add(1)
                > max_path_bytes
            {
                continue;
            }
            paths.push_str(separator);
            paths.push_str(&encoded_path);
        }
        paths.push(']');
        format!("{TRUSTED_SKILLS_PREFIX}{paths}")
    }
}

/// Host-owned roots from which invoked skill paths can be verified.
pub(crate) struct TrustedSkillRoots {
    roots: Vec<PathBuf>,
}

impl TrustedSkillRoots {
    pub(crate) fn from_config(config: &Config) -> Self {
        let mut roots = vec![config.codex_home.join("skills").to_path_buf()];
        if let Some(user_home) = home_dir() {
            roots.push(user_home.join(".agents").join("skills"));
        }
        Self { roots }
    }

    pub(crate) fn trusted_skill_path(&self, skill_resource: &str) -> Option<String> {
        let skill_path = Path::new(skill_resource).canonicalize().ok()?;
        if !self.roots.iter().any(|root| {
            root.canonicalize()
                .is_ok_and(|trusted_root| skill_path.starts_with(trusted_root))
        }) {
            return None;
        }

        let path = skill_path.to_str()?.to_owned();
        if path.len() > MAX_TRUSTED_SKILL_PATH_BYTES || !skill_path.is_file() {
            return None;
        }

        Some(path)
    }
}

/// Bounded, deduplicated user-owned skill paths observed in one turn.
#[derive(Default)]
pub(crate) struct TrustedSkillInvocations(BTreeSet<String>);

impl TrustedSkillInvocations {
    pub(crate) fn record(&mut self, path: String) {
        let skills = &mut self.0;
        if skills.contains(&path)
            || skills.len() >= MAX_TRUSTED_SKILLS
            || skills
                .iter()
                .map(String::len)
                .sum::<usize>()
                .saturating_add(path.len())
                > MAX_TRUSTED_SKILL_PATHS_BYTES
        {
            return;
        }
        skills.insert(path);
    }

    pub(crate) fn into_paths(self) -> Vec<String> {
        self.0.into_iter().collect()
    }
}

#[cfg(test)]
#[path = "trusted_skills_tests.rs"]
mod tests;

use std::path::Path;

use super::CronTask;

pub(super) fn generate_unique_id(tasks: &[CronTask]) -> Result<String, String> {
    for _ in 0..8 {
        let id = format!("{:08x}", rand::random::<u32>());
        if !tasks.iter().any(|task| task.id == id) {
            return Ok(id);
        }
    }
    Err("SessionCronStore: failed to generate a unique 8-hex id after 8 attempts".to_string())
}

pub(super) fn is_valid_id(id: &str) -> bool {
    id.len() == 8
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) async fn persist_task(storage_dir: &Path, task: &CronTask) -> std::io::Result<()> {
    let path = storage_dir.join(format!("{}.json", task.id));
    let contents = serde_json::to_string_pretty(task).map_err(std::io::Error::other)?;
    tokio::task::spawn_blocking(move || crate::path_utils::write_atomically(&path, &contents))
        .await
        .map_err(std::io::Error::other)?
}

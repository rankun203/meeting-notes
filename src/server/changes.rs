//! Portable file-change reconciliation. A one-second metadata poll is deliberately
//! used instead of platform-specific watcher delivery: atomic editor saves,
//! imported directories and missed events all follow the same path. Audio files
//! are excluded; this never reads transcript or conversation bodies.
use super::routes::AppState;
use crate::session::ServerEvent;
use crate::storage::{self, Revision};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

fn snapshot(root: &Path) -> HashMap<PathBuf, Revision> {
    let mut files = Vec::new();
    for section in ["recordings", "people", "conversations"] {
        let dir = root.join(section);
        files.extend(
            std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .flat_map(|entry| {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        std::fs::read_dir(entry.path())
                            .into_iter()
                            .flatten()
                            .flatten()
                            .map(|e| e.path())
                            .collect()
                    } else {
                        vec![entry.path()]
                    }
                }),
        );
    }
    files.push(root.join("tags.json"));
    files
        .into_iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .filter_map(|p| {
            storage::revision(&p).map(|r| (p.strip_prefix(root).unwrap().to_path_buf(), r))
        })
        .collect()
}

pub fn start(state: AppState) {
    tokio::spawn(async move {
        let root = state
            .files_db
            .recordings_dir()
            .parent()
            .unwrap()
            .to_path_buf();
        let path = root.clone();
        let mut previous = storage::blocking(move || snapshot(&path)).await;
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            timer.tick().await;
            let path = root.clone();
            let next = storage::blocking(move || snapshot(&path)).await;
            let changed: HashSet<_> = previous
                .keys()
                .chain(next.keys())
                .filter(|p| previous.get(*p) != next.get(*p))
                .cloned()
                .collect();
            if !changed.is_empty() {
                let mut sessions = HashSet::new();
                let mut people = false;
                let mut tags = false;
                let mut conversations = false;
                for path in changed {
                    let parts: Vec<_> = path.iter().filter_map(|s| s.to_str()).collect();
                    match parts.as_slice() {
                        ["recordings", id, _] => {
                            sessions.insert((*id).to_string());
                        }
                        ["people", ..] => people = true,
                        ["conversations", ..] => conversations = true,
                        ["tags.json"] => tags = true,
                        _ => {}
                    }
                }
                state.session_manager.reconcile().await;
                if people {
                    state.people_manager.load_from_disk().await;
                }
                state.session_manager.emit(ServerEvent::FilesChanged {
                    sessions: sessions.into_iter().collect(),
                    people,
                    tags,
                    conversations,
                });
            }
            previous = next;
        }
    });
}

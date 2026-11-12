use crate::library::Library;
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};
const MAX_RECENT: usize = 5;

#[derive(Serialize, Deserialize)]
struct LastLibrary {
    root: String,
    /// Most recent first, including `root`.
    #[serde(default)]
    recent: Vec<String>,
}

fn read(config: &Path) -> Option<LastLibrary> {
    serde_json::from_slice(&fs::read(config.join("last_library.json")).ok()?).ok()
}

/// Recently opened libraries that still exist, most recent first.
pub fn recent(config: &Path) -> Vec<String> {
    read(config)
        .map(|record| record.recent)
        .unwrap_or_default()
        .into_iter()
        .filter(|root| Path::new(root).join("library.json").is_file())
        .collect()
}

pub fn remember(config: &Path, root: &str) -> Result<(), String> {
    fs::create_dir_all(config).map_err(|e| format!("无法创建应用配置目录：{e}"))?;
    let mut recent = read(config).map(|record| record.recent).unwrap_or_default();
    recent.retain(|item| item != root);
    recent.insert(0, root.into());
    recent.truncate(MAX_RECENT);
    let mut temp = tempfile::NamedTempFile::new_in(config).map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut temp, &LastLibrary { root: root.into(), recent })
        .map_err(|e| e.to_string())?;
    temp.flush().map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(config.join("last_library.json"))
        .map_err(|e| format!("无法记住资料库：{e}"))?;
    Ok(())
}
pub fn reopen(
    config: &Path,
    authorize: impl FnOnce(&Library) -> Result<(), String>,
) -> Option<Library> {
    let path = config.join("last_library.json");
    let result = (|| {
        let record = read(config)?;
        let library = Library::open(Path::new(&record.root)).ok()?;
        authorize(&library).ok()?;
        Some(library)
    })();
    if result.is_none() {
        // Keep the recent list; only forget which library to reopen.
        let recent = recent(config);
        if recent.is_empty() {
            let _ = fs::remove_file(&path);
        } else if let Ok(bytes) = serde_json::to_vec(&LastLibrary { root: String::new(), recent }) {
            let _ = fs::write(&path, bytes);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn last_library_reopens_and_invalid_record_is_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        let library = Library::create(dir.path()).unwrap();
        let root = library.snapshot().root;
        remember(&config, &root).unwrap();
        drop(library);
        assert_eq!(reopen(&config, |_| Ok(())).unwrap().snapshot().root, root);
        remember(&config, "/does-not-exist").unwrap();
        assert!(reopen(&config, |_| Ok(())).is_none());
        assert_eq!(recent(&config), vec![root.clone()], "missing libraries drop out of the recent list");
        for i in 0..7 {
            let other = dir.path().join(format!("lib{i}"));
            fs::create_dir(&other).unwrap();
            let lib = Library::create_at(&other).unwrap();
            remember(&config, &lib.snapshot().root).unwrap();
        }
        remember(&config, &root).unwrap();
        let list = recent(&config);
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0], root);
        assert_eq!(list.iter().filter(|item| **item == root).count(), 1);
    }
}

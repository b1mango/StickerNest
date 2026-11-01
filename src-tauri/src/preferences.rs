use crate::library::Library;
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};
#[derive(Serialize, Deserialize)]
struct LastLibrary {
    root: String,
}

pub fn remember(config: &Path, root: &str) -> Result<(), String> {
    fs::create_dir_all(config).map_err(|e| format!("无法创建应用配置目录：{e}"))?;
    let mut temp = tempfile::NamedTempFile::new_in(config).map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut temp, &LastLibrary { root: root.into() })
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
        let bytes = fs::read(&path).ok()?;
        let record: LastLibrary = serde_json::from_slice(&bytes).ok()?;
        let library = Library::open(Path::new(&record.root)).ok()?;
        authorize(&library).ok()?;
        Some(library)
    })();
    if result.is_none() {
        let _ = fs::remove_file(path);
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
        assert!(!config.join("last_library.json").exists());
    }
}

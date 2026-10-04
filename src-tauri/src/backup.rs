//! Full local library snapshots. Backup copies the indexed assets, library.json and
//! management.json into a new directory and verifies every hash before marking the
//! manifest complete. Restore copies a complete backup into a new directory, verifies
//! hashes again, then validates the result by opening it as a library.
use crate::library::Library;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 20 * 1024 * 1024;
const MAX_FILES: usize = 20_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupManifest {
    pub version: u32,
    pub created_at: u64,
    pub complete: bool,
    pub asset_count: usize,
    pub total_bytes: u64,
    pub files: Vec<BackupFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub path: String,
    pub asset_count: usize,
    pub total_bytes: u64,
    pub includes_management: bool,
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn now() -> Result<u64, String> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(err)?
        .as_secs())
}

/// UTC timestamp label for backup folder names, e.g. 20261004-073000.
fn timestamp_label(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let secs_of_day = secs % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60
    )
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn hash_valid(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|e| format!("无法读取 {}：{e}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err("备份只接受普通文件，不能接受符号链接".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| format!("无法读取 {}：{e}", path.display()))?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("无法读取 {}：{e}", path.display()))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("资料库文件超过 20 MiB 限制".into());
    }
    Ok(bytes)
}

/// Write bytes to a fresh destination and verify the persisted content by re-reading it.
fn write_verified(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or("备份目标路径无效".to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("无法在目标位置写入：{e}"))?;
    temp.write_all(bytes)
        .map_err(|e| format!("无法写入 {}：{e}", destination.display()))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| format!("无法写入 {}：{e}", destination.display()))?;
    temp.persist_noclobber(destination)
        .map_err(|e| format!("无法保存 {}：{e}", destination.display()))?;
    let persisted = read_bounded(destination)?;
    if persisted != bytes {
        return Err("备份写入校验失败，目标内容与来源不一致".into());
    }
    Ok(())
}

fn valid_entry_path(path: &str) -> bool {
    if path == "library.json" || path == "management.json" {
        return true;
    }
    let Some(name) = path.strip_prefix("assets/") else {
        return false;
    };
    let Some((id, ext)) = name.rsplit_once('.') else {
        return false;
    };
    hash_valid(id) && matches!(ext, "png" | "jpg" | "gif" | "webp")
}

fn write_manifest(target: &Path, manifest: &BackupManifest) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(manifest).map_err(err)?;
    let mut temp = tempfile::NamedTempFile::new_in(target)
        .map_err(|e| format!("无法写入备份清单：{e}"))?;
    temp.write_all(&data)
        .map_err(|e| format!("无法写入备份清单：{e}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| format!("无法写入备份清单：{e}"))?;
    temp.persist(target.join("backup-manifest.json"))
        .map_err(|e| format!("无法保存备份清单：{e}"))?;
    Ok(())
}

impl Library {
    /// Copy the consistent, locked state of this library into a new backup directory.
    pub fn backup_to(&self, target_parent: &Path) -> Result<BackupSummary, String> {
        let root = PathBuf::from(self.snapshot().root);
        // A backup must stay restorable; refuse to produce one restore would reject.
        if self.snapshot().items.len() + 2 > MAX_FILES {
            return Err("素材数量超过当前备份上限，请先联系维护者调整限制".into());
        }
        let parent = target_parent
            .canonicalize()
            .map_err(|e| format!("无法访问所选文件夹：{e}"))?;
        if !parent.is_dir() {
            return Err("请选择备份存放的文件夹".into());
        }
        if parent == root || parent.starts_with(&root) {
            return Err("备份不能放在资料库内部".into());
        }
        let target = parent.join(format!("StickerNest Backup {}", timestamp_label(now()?)));
        fs::create_dir(&target)
            .map_err(|e| format!("无法创建备份文件夹（不会覆盖已有备份）：{e}"))?;
        let mut files = Vec::new();
        let result: Result<bool, String> = (|| {
            fs::create_dir(target.join("assets"))
                .map_err(|e| format!("无法创建备份目录：{e}"))?;
            let mut entries: Vec<(PathBuf, String)> = vec![(
                root.join("library.json"),
                "library.json".to_string(),
            )];
            let management = root.join("management.json");
            let includes_management = match fs::symlink_metadata(&management) {
                Ok(meta) if meta.file_type().is_file() => true,
                Ok(_) => return Err("整理记录文件异常（不是普通文件），备份已停止".into()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(e) => return Err(format!("无法读取整理记录：{e}")),
            };
            if includes_management {
                entries.push((management, "management.json".to_string()));
            }
            for item in self.snapshot().items {
                entries.push((
                    root.join("assets").join(&item.file_name),
                    format!("assets/{}", item.file_name),
                ));
            }
            for (source, relative) in entries {
                let bytes = read_bounded(&source)?;
                files.push(BackupFile {
                    path: relative,
                    sha256: hash(&bytes),
                    bytes: bytes.len() as u64,
                });
                write_verified(&target.join(&files.last().unwrap().path), &bytes)?;
            }
            Ok(includes_management)
        })();
        let includes_management = match result {
            Ok(value) => value,
            Err(error) => {
                // Keep the partial directory as an explicitly incomplete backup.
                let _ = write_manifest(
                    &target,
                    &BackupManifest {
                        version: 1,
                        created_at: now()?,
                        complete: false,
                        asset_count: 0,
                        total_bytes: 0,
                        files,
                    },
                );
                return Err(format!("备份未完成，已保留部分备份供排查：{error}"));
            }
        };
        let asset_count = self.snapshot().items.len();
        let total_bytes = files.iter().map(|file| file.bytes).sum();
        write_manifest(
            &target,
            &BackupManifest {
                version: 1,
                created_at: now()?,
                complete: true,
                asset_count,
                total_bytes,
                files,
            },
        )?;
        Ok(BackupSummary {
            path: target.to_string_lossy().into_owned(),
            asset_count,
            total_bytes,
            includes_management,
        })
    }
}

fn validate_manifest(manifest: &BackupManifest) -> Result<(), String> {
    if manifest.version != 1
        || !manifest.complete
        || manifest.files.is_empty()
        || manifest.files.len() > MAX_FILES
        || manifest.created_at == 0
    {
        return Err("备份清单无效或备份未完成，不能恢复".into());
    }
    let mut seen = HashSet::new();
    let mut assets = 0;
    let mut bytes = 0_u64;
    for file in &manifest.files {
        if !valid_entry_path(&file.path)
            || !hash_valid(&file.sha256)
            || file.bytes == 0
            || file.bytes > MAX_BYTES
            || !seen.insert(file.path.as_str())
        {
            return Err("备份清单含无效或不安全的文件记录".into());
        }
        if file.path.starts_with("assets/") {
            assets += 1;
        }
        bytes += file.bytes;
    }
    if assets != manifest.asset_count || bytes != manifest.total_bytes {
        return Err("备份清单统计与文件记录不一致".into());
    }
    Ok(())
}

/// Restore a complete, verified backup into `target_parent/StickerNest Library`.
/// Returns the restored library root. Never overwrites an existing directory.
pub fn restore(backup_dir: &Path, target_parent: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(backup_dir)
        .map_err(|e| format!("无法访问备份文件夹：{e}"))?;
    if !metadata.file_type().is_dir() {
        return Err("备份必须是实际文件夹，不能是符号链接".into());
    }
    let backup = backup_dir
        .canonicalize()
        .map_err(|e| format!("无法访问备份文件夹：{e}"))?;
    let parent = target_parent
        .canonicalize()
        .map_err(|e| format!("无法访问恢复位置：{e}"))?;
    if !parent.is_dir() {
        return Err("请选择恢复到的文件夹".into());
    }
    if parent == backup || parent.starts_with(&backup) {
        return Err("不能恢复到备份文件夹内部".into());
    }
    let manifest: BackupManifest = serde_json::from_slice(
        &read_bounded(&backup.join("backup-manifest.json"))
            .map_err(|_| "所选文件夹不是有效的拾趣备份（缺少 backup-manifest.json）".to_string())?,
    )
    .map_err(|e| format!("备份清单损坏，未执行恢复：{e}"))?;
    validate_manifest(&manifest)?;
    let target = parent.join("StickerNest Library");
    fs::create_dir(&target)
        .map_err(|e| format!("无法创建恢复目录（不会覆盖现有文件夹）：{e}"))?;
    if let Err(error) = fs::create_dir(target.join("assets")) {
        let _ = fs::remove_dir_all(&target);
        return Err(format!("无法创建恢复目录：{error}"));
    }
    let result: Result<(), String> = (|| {
        for file in &manifest.files {
            let bytes = read_bounded(&backup.join(&file.path))?;
            if bytes.len() as u64 != file.bytes || hash(&bytes) != file.sha256 {
                return Err(format!("备份文件与清单不一致，已停止恢复：{}", file.path));
            }
            if let Some(name) = file.path.strip_prefix("assets/") {
                let id = name.rsplit_once('.').map(|(id, _)| id).unwrap_or("");
                if hash(&bytes) != id {
                    return Err("备份素材内容与文件名不符，已停止恢复".into());
                }
            }
            write_verified(&target.join(&file.path), &bytes)?;
        }
        // Validate the restored library, including its management records, before success.
        let library = Library::open(&target)?;
        library.get_management()?;
        drop(library);
        Ok(())
    })();
    if let Err(error) = result {
        // The restore target was created by this call and contains only our copies.
        let _ = fs::remove_dir_all(&target);
        return Err(error);
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Library, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::create(dir.path()).unwrap();
        let mut paths = vec![];
        for (i, color) in [[255, 0, 0, 255], [0, 0, 255, 255]].iter().enumerate() {
            let p = dir.path().join(format!("{i}.png"));
            image::RgbaImage::from_pixel(2, 2, image::Rgba(*color))
                .save(&p)
                .unwrap();
            paths.push(p);
        }
        lib.import_files(paths, "本地".into()).unwrap();
        let ids = lib.snapshot().items.iter().map(|i| i.id.clone()).collect();
        (dir, lib, ids)
    }
    fn bytes(path: &Path) -> Vec<u8> {
        fs::read(path).unwrap()
    }
    #[test]
    fn timestamp_label_is_utc() {
        assert_eq!(timestamp_label(0), "19700101-000000");
        assert_eq!(timestamp_label(86_400), "19700102-000000");
    }
    #[test]
    fn backup_and_restore_roundtrip_preserves_every_byte() {
        let (dir, lib, ids) = fixture();
        let root = PathBuf::from(lib.snapshot().root);
        lib.save_metadata(&root.to_string_lossy(), ids[0].clone(), "测试".into(), vec!["标签".into()], vec![])
            .unwrap();
        let summary = lib.backup_to(dir.path()).unwrap();
        let backup = PathBuf::from(summary.path);
        assert_eq!(summary.asset_count, 2);
        assert!(summary.includes_management);
        let manifest: BackupManifest =
            serde_json::from_slice(&bytes(&backup.join("backup-manifest.json"))).unwrap();
        assert!(manifest.complete);
        assert_eq!(manifest.files.len(), 4);
        for file in &manifest.files {
            let copied = bytes(&backup.join(&file.path));
            assert_eq!(copied.len() as u64, file.bytes);
            assert_eq!(hash(&copied), file.sha256);
            assert_eq!(copied, bytes(&root.join(&file.path)));
        }
        let restore_parent = tempfile::tempdir().unwrap();
        let restored = restore(&backup, restore_parent.path()).unwrap();
        assert_eq!(restored.file_name().unwrap(), "StickerNest Library");
        assert_eq!(bytes(&restored.join("library.json")), bytes(&root.join("library.json")));
        assert_eq!(
            bytes(&restored.join("management.json")),
            bytes(&root.join("management.json"))
        );
        let reopened = Library::open(&restored).unwrap();
        assert_eq!(reopened.snapshot().items.len(), 2);
        assert_eq!(reopened.get_management().unwrap().metadata[&ids[0]].name, "测试");
    }
    #[test]
    fn backup_rejects_target_inside_library() {
        let (_dir, lib, _ids) = fixture();
        let root = PathBuf::from(lib.snapshot().root);
        assert!(lib.backup_to(&root).is_err());
        assert!(lib.backup_to(&root.join("assets")).is_err());
    }
    #[test]
    fn restore_rejects_incomplete_tampered_and_traversing_backups() {
        let (dir, lib, _ids) = fixture();
        let summary = lib.backup_to(dir.path()).unwrap();
        let backup = PathBuf::from(summary.path);
        let manifest_path = backup.join("backup-manifest.json");
        let manifest: BackupManifest = serde_json::from_slice(&bytes(&manifest_path)).unwrap();
        let restore_parent = tempfile::tempdir().unwrap();
        // Incomplete backups are rejected.
        let mut incomplete = manifest.clone();
        incomplete.complete = false;
        fs::write(&manifest_path, serde_json::to_vec(&incomplete).unwrap()).unwrap();
        assert!(restore(&backup, restore_parent.path()).is_err());
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        // Path traversal entries are rejected.
        let mut evil = manifest.clone();
        evil.files.push(BackupFile {
            path: "../evil".into(),
            sha256: hash(b"x"),
            bytes: 1,
        });
        fs::write(&manifest_path, serde_json::to_vec(&evil).unwrap()).unwrap();
        assert!(restore(&backup, restore_parent.path()).is_err());
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        // A tampered asset no longer matches its recorded hash.
        let asset = manifest
            .files
            .iter()
            .find(|file| file.path.starts_with("assets/"))
            .unwrap();
        let target = backup.join(&asset.path);
        let mut tampered = bytes(&target);
        tampered[0] ^= 0xff;
        fs::write(&target, tampered).unwrap();
        assert!(restore(&backup, restore_parent.path()).is_err());
        // A failed restore removes only the partial directory it created itself.
        assert!(!restore_parent.path().join("StickerNest Library").exists());
    }
    #[test]
    fn restore_rejects_existing_target_and_nested_locations() {
        let (dir, lib, _ids) = fixture();
        let summary = lib.backup_to(dir.path()).unwrap();
        let backup = PathBuf::from(summary.path);
        let parent = tempfile::tempdir().unwrap();
        fs::create_dir(parent.path().join("StickerNest Library")).unwrap();
        assert!(restore(&backup, parent.path()).is_err());
        assert!(restore(&backup, &backup).is_err());
        let inside = backup.join("sub");
        fs::create_dir(&inside).unwrap();
        assert!(restore(&backup, &inside).is_err());
    }
    #[test]
    fn restore_rejects_semantically_corrupt_management() {
        let (dir, lib, ids) = fixture();
        let root = PathBuf::from(lib.snapshot().root);
        lib.save_metadata(&root.to_string_lossy(), ids[0].clone(), "测试".into(), vec![], vec![])
            .unwrap();
        let summary = lib.backup_to(dir.path()).unwrap();
        let backup = PathBuf::from(summary.path);
        // Valid JSON with matching manifest hashes, but management content the app rejects.
        let corrupt = serde_json::to_vec(&serde_json::json!({
            "version": 9, "metadata": {}, "accounts": [], "references": [], "batches": [], "trash": {}
        }))
        .unwrap();
        fs::write(backup.join("management.json"), &corrupt).unwrap();
        let manifest_path = backup.join("backup-manifest.json");
        let mut manifest: BackupManifest =
            serde_json::from_slice(&bytes(&manifest_path)).unwrap();
        let entry = manifest
            .files
            .iter_mut()
            .find(|file| file.path == "management.json")
            .unwrap();
        entry.sha256 = hash(&corrupt);
        manifest.total_bytes = manifest.total_bytes - entry.bytes + corrupt.len() as u64;
        entry.bytes = corrupt.len() as u64;
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let restore_parent = tempfile::tempdir().unwrap();
        let error = restore(&backup, restore_parent.path()).unwrap_err();
        assert!(!restore_parent.path().join("StickerNest Library").exists());
        assert!(!error.contains("清单"));
    }
    #[cfg(unix)]
    #[test]
    fn backup_refuses_symlink_management_file() {
        let (dir, lib, ids) = fixture();
        let root = PathBuf::from(lib.snapshot().root);
        lib.save_metadata(&root.to_string_lossy(), ids[0].clone(), "测试".into(), vec![], vec![])
            .unwrap();
        let link_target = dir.path().join("elsewhere.json");
        fs::rename(root.join("management.json"), &link_target).unwrap();
        std::os::unix::fs::symlink(&link_target, root.join("management.json")).unwrap();
        let error = lib.backup_to(dir.path()).unwrap_err();
        assert!(error.contains("整理记录"));
    }
    #[test]
    fn restore_reports_missing_manifest_in_chinese() {
        let dir = tempfile::tempdir().unwrap();
        let not_backup = dir.path().join("random-folder");
        fs::create_dir(&not_backup).unwrap();
        let error = restore(&not_backup, dir.path()).unwrap_err();
        assert!(error.contains("不是有效的拾趣备份"));
    }
}

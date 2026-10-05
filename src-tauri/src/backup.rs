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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub path: String,
    pub exported: usize,
    pub skipped: usize,
}

impl Library {
    /// Export assets to a fresh StickerNest Export folder. Original bytes and
    /// format are preserved; renames never overwrite; trash is excluded.
    /// The Tauri command checks expected_root before delegating here.
    pub fn export_assets(
        &self,
        target_parent: &Path,
        asset_ids: &[String],
        name_map: &std::collections::HashMap<String, String>,
    ) -> Result<ExportSummary, String> {
        let root = PathBuf::from(self.snapshot().root);
        let parent = target_parent
            .canonicalize()
            .map_err(|e| format!("无法访问所选文件夹：{e}"))?;
        if !parent.is_dir() {
            return Err("请选择导出存放的文件夹".into());
        }
        if parent == root || parent.starts_with(&root) {
            return Err("不能导出到资料库内部".into());
        }
        if asset_ids.is_empty() || asset_ids.len() > MAX_FILES {
            return Err("一次最多导出 20000 个素材".into());
        }
        let management = self.get_management()?;
        let mut skipped = 0_usize;
        let mut live: Vec<&String> = vec![];
        let mut seen = HashSet::new();
        for id in asset_ids {
            if !hash_valid(id) || !seen.insert(id) {
                skipped += 1;
                continue;
            }
            if management.trash.contains_key(id) {
                skipped += 1;
                continue;
            }
            live.push(id);
        }
        let stamped = timestamp_label(now()?);
        let mut target = parent.join(format!("StickerNest Export {stamped}"));
        let mut suffix = 1;
        // Timestamp collisions append a suffix; existing folders are never reused.
        while target.exists() {
            target = parent.join(format!("StickerNest Export {stamped}-{suffix}"));
            suffix += 1;
        }
        fs::create_dir(&target)
            .map_err(|e| format!("无法创建导出文件夹（不会覆盖已有文件夹）：{e}"))?;
        for id in &live {
            let source = self.asset_path(id)?;
            let bytes = read_bounded(&source)?;
            let item = self
                .snapshot()
                .items
                .into_iter()
                .find(|item| item.id == **id)
                .ok_or("素材不存在")?;
            let extension = item.format;
            let base = name_map
                .get(*id)
                .cloned()
                .unwrap_or_else(|| item.name.trim_end_matches(&format!(".{extension}")).to_string());
            // Strip path separators, ASCII controls, and Unicode format
            // characters (bidi overrides, zero-width, BOM) that can spoof
            // extensions or make names look identical while differing.
            let base = sanitize_name(&base);
            // macOS limits file names to 255 UTF-8 bytes; leave room for the
            // "-N" suffix and the extension.
            let name_budget = 250_usize.saturating_sub(extension.len() + 1);
            let base = truncate_utf8(base.trim().trim_end_matches('.').trim(), name_budget);
            let base = if base.is_empty() || base.chars().all(|c| c == '.') {
                id[..12].to_string()
            } else {
                base.to_string()
            };
            let mut file_name = format!("{base}.{extension}");
            let mut destination = target.join(&file_name);
            let mut suffix = 1;
            while fs::symlink_metadata(&destination).is_ok() {
                file_name = format!("{base}-{suffix}.{extension}");
                destination = target.join(&file_name);
                suffix += 1;
            }
            write_verified(&destination, &bytes)?;
        }
        Ok(ExportSummary {
            path: target.to_string_lossy().into_owned(),
            exported: live.len(),
            skipped,
        })
    }
}

/// Remove characters that could spoof names across filesystems or displays.
fn sanitize_name(name: &str) -> String {
    name.chars()
        .filter(|c| {
            !"/\\:*?\"<>|\r\n\t".contains(*c)
                && !c.is_control()
                && !matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}')
        })
        .collect()
}

/// Truncate to at most `max_bytes` UTF-8 bytes without splitting a character.
fn truncate_utf8(name: &str, max_bytes: usize) -> &str {
    if name.len() <= max_bytes {
        return name;
    }
    let mut end = max_bytes;
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name[..end]
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectStageResult {
    pub stage: String,
    pub detail: String,
}

impl Library {
    /// Stage 1 of the in-app Douyin collection: run the local node collector
    /// (Chrome DevTools) and the resumable python downloader. Everything lands
    /// under downloads/ inside the library; assets are imported in stage 2.
    pub fn collect_douyin_fetch(
        &self,
        scripts_dir: &Path,
        chrome_port: u16,
    ) -> Result<CollectStageResult, String> {
        let root = PathBuf::from(self.snapshot().root);
        let downloads = root.join("downloads");
        fs::create_dir_all(&downloads).map_err(|e| format!("无法创建采集目录：{e}"))?;
        let list_path = downloads.join("stickers.json");
        let collector = scripts_dir.join("collect_douyin.mjs");
        if !collector.is_file() {
            return Err(format!(
                "缺少采集脚本 collect_douyin.mjs（搜索位置：{}）。请用 npm run desktop 从项目目录启动应用。",
                scripts_dir.display()
            ));
        }
        let output = std::process::Command::new("node")
            .arg(&collector)
            .arg("--out")
            .arg(&list_path)
            .arg("--port")
            .arg(chrome_port.to_string())
            .output()
            .map_err(|e| format!("无法启动 node（请先安装 Node.js）：{e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("收藏清单读取未完成：{}", stderr.trim()));
        }
        let stickers: Vec<serde_json::Value> =
            serde_json::from_slice(&fs::read(&list_path).map_err(|e| format!("清单文件读取失败：{e}"))?)
                .map_err(|e| format!("清单文件不是有效列表：{e}"))?;
        if stickers.is_empty() {
            return Err("收藏清单为空".into());
        }
        let downloader = scripts_dir.join("download_douyin.py");
        if !downloader.is_file() {
            return Err("缺少下载脚本 download_douyin.py".into());
        }
        let output = std::process::Command::new("python3")
            .arg(&downloader)
            .arg(&list_path)
            .arg(&downloads)
            .output()
            .map_err(|e| format!("无法启动 python3：{e}"))?;
        if !output.status.success() {
            // The downloader prints per-item and summary failures to stdout.
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let detail = if stderr.trim().is_empty() {
                stdout.trim().to_string()
            } else if stdout.trim().is_empty() {
                stderr.trim().to_string()
            } else {
                format!("{}\n{}", stderr.trim(), stdout.trim())
            };
            return Err(format!("资源下载未完成：{}", detail.chars().take(600).collect::<String>()));
        }
        Ok(CollectStageResult {
            stage: "fetch".into(),
            detail: format!("清单 {} 项，原件已下载到 downloads（中断可重试）", stickers.len()),
        })
    }

    /// Stage 1 of WeChat manifest download: plain URL list from wxemoticon,
    /// staged under downloads/wechat/ so it never collides with Douyin data.
    pub fn import_wechat_manifest(
        &self,
        scripts_dir: &Path,
        manifest_path: &Path,
    ) -> Result<CollectStageResult, String> {
        if !manifest_path.is_file() {
            return Err("清单文件不存在，请选择 wxemoticon 导出的 emoticon_urls.txt".into());
        }
        let downloader = scripts_dir.join("download_wechat.py");
        if !downloader.is_file() {
            return Err(format!(
                "缺少下载脚本 download_wechat.py（搜索位置：{}）。请用 npm run desktop 从项目目录启动应用。",
                scripts_dir.display()
            ));
        }
        let downloads = PathBuf::from(self.snapshot().root).join("downloads").join("wechat");
        fs::create_dir_all(&downloads).map_err(|e| format!("无法创建下载目录：{e}"))?;
        let manifest_dest = downloads.join("manifest.txt");
        fs::copy(manifest_path, &manifest_dest)
            .map_err(|e| format!("无法保存清单副本：{e}"))?;
        let output = std::process::Command::new("python3")
            .arg(&downloader)
            .arg(&manifest_dest)
            .arg(&downloads)
            .output()
            .map_err(|e| format!("无法启动 python3:{e}"))?;
        // exit 2 = partial failure: still acceptable when something downloaded.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let summary: serde_json::Value = serde_json::from_str(stdout.trim().lines().last().unwrap_or(""))
            .map_err(|_| format!("清单下载脚本输出无法解析：{}", stdout.trim().chars().take(400).collect::<String>()))?;
        let verified = summary["verified"].as_u64().unwrap_or(0);
        if verified == 0 {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = format!("{}\n{}", stderr.trim(), stdout.trim());
            return Err(format!("清单全部下载失败：{}", detail.chars().take(600).collect::<String>()));
        }
        Ok(CollectStageResult {
            stage: "fetch".into(),
            detail: format!(
                "清单 {} 项 · 下载成功 {} · 失败 {}（原件保存在库内 downloads/wechat)",
                summary["total"], summary["verified"], summary["failed"]
            ),
        })
    }

    /// Stage 2 of WeChat import: originals referenced by the staged report
    /// (verified items only), source "微信". Returns the report for mapping.
    pub fn collect_wechat_import(&mut self) -> Result<(crate::library::ImportReport, PathBuf), String> {
        let wechat = PathBuf::from(self.snapshot().root)
            .join("downloads")
            .join("wechat");
        let report_path = wechat.join("report.json");
        #[derive(serde::Deserialize)]
        struct StagedReport {
            items: Vec<StagedItem>,
        }
        #[derive(serde::Deserialize)]
        struct StagedItem {
            status: String,
            #[serde(default)]
            sha256: Option<String>,
            #[serde(default)]
            format: Option<String>,
        }
        let staged_bytes = fs::read(&report_path)
            .map_err(|e| format!("请先完成清单下载:{e}"))?;
        let staged: StagedReport = serde_json::from_slice(&staged_bytes)
            .map_err(|e| format!("下载报告无法解析:{e}"))?;
        let mut paths: Vec<PathBuf> = staged
            .items
            .iter()
            .filter(|item| item.status == "verified")
            .filter_map(|item| match (&item.sha256, &item.format) {
                (Some(sha), Some(fmt)) => Some(wechat.join("originals").join(format!("{sha}.{fmt}"))),
                _ => None,
            })
            .collect();
        if paths.is_empty() {
            return Err("清单中没有下载成功的原件".into());
        }
        paths.sort();
        let report = self.import_files(paths, "微信".into())?;
        Ok((report, report_path))
    }

    /// Stage 2: import the staged originals and return the report path for
    /// account mapping (import_provenance handles the account step in the UI).
    pub fn collect_douyin_import(&mut self) -> Result<(crate::library::ImportReport, PathBuf), String> {
        let downloads = PathBuf::from(self.snapshot().root).join("downloads");
        let originals = downloads.join("originals");
        let mut paths: Vec<PathBuf> = fs::read_dir(&originals)
            .map_err(|e| format!("下载原件目录不存在，请先执行采集：{e}"))?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect();
        paths.sort();
        if paths.is_empty() {
            return Err("downloads 中没有原件，请先执行采集".into());
        }
        // Safe local import of files this app downloaded itself.
        let report = self.import_files(paths, "抖音".into())?;
        Ok((report, downloads.join("report.json")))
    }
}

#[cfg(test)]
mod tests {
    static PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
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
    #[test]
    fn export_preserves_bytes_never_overwrites_and_skips_trash() {
        let (dir, lib, ids) = fixture();
        let root = PathBuf::from(lib.snapshot().root);
        lib.set_trash(&root.to_string_lossy(), vec![ids[0].clone()], true)
            .unwrap();
        let names = std::collections::HashMap::from([
            (ids[0].clone(), "开心".to_string()),
            (ids[1].clone(), "开心".to_string()),
        ]);
        let summary = lib
            .export_assets(dir.path(), &ids, &names)
            .unwrap();
        assert_eq!(summary.exported, 1);
        assert_eq!(summary.skipped, 1);
        let first_path = summary.path.clone();
        let target = std::path::PathBuf::from(&summary.path);
        let file = std::fs::read_dir(&target).unwrap().next().unwrap().unwrap();
        assert!(file.file_name().to_string_lossy().starts_with("开心"));
        // Bytes are preserved exactly.
        assert_eq!(
            std::fs::read(file.path()).unwrap(),
            std::fs::read(lib.asset_path(&ids[1]).unwrap()).unwrap()
        );
        // A second export produces another folder, never overwrites.
        let summary2 = lib
            .export_assets(dir.path(), &ids, &names)
            .unwrap();
        assert_ne!(first_path, summary2.path);
        assert!(lib
            .export_assets(&root, &ids, &names)
            .is_err());
    }
    #[test]
    fn collect_douyin_import_stages_originals_and_returns_report_path() {
        let (dir, mut lib, _ids0) = fixture();
        let root = PathBuf::from(lib.snapshot().root);
        let originals = root.join("downloads").join("originals");
        fs::create_dir_all(&originals).unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([9, 9, 9, 255]))
            .save(originals.join("a.png"))
            .unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([8, 8, 8, 255]))
            .save(originals.join("b.png"))
            .unwrap();
        fs::write(root.join("downloads").join("report.json"), b"{}").unwrap();
        let (report, report_path) = lib.collect_douyin_import().unwrap();
        assert_eq!(report.added, 2);
        assert!(report_path.ends_with("downloads/report.json"));
        assert_eq!(lib.snapshot().items.len(), 4);
        assert!(lib
            .snapshot()
            .items
            .iter()
            .all(|item| item.sources.contains(&"抖音".to_string()) || item.sources.contains(&"本地".to_string())));
        // Missing originals is a clear error, not an empty success.
        let dir2 = tempfile::tempdir().unwrap();
        let mut lib2 = Library::create(dir2.path()).unwrap();
        assert!(lib2.collect_douyin_import().is_err());
        let _ = dir;
    }
    #[cfg(unix)]
    #[test]
    fn collect_pipeline_runs_collector_then_downloader() {
        let _path_guard = PATH_LOCK.lock().unwrap();
        use std::os::unix::fs::PermissionsExt;
        // Shim "node" records the invocation and writes a two-sticker list.
        let (dir, mut lib, _ids) = fixture();
        let root = PathBuf::from(lib.snapshot().root);
        let shim = dir.path().join("shim");
        fs::create_dir(&shim).unwrap();
        let node_shim = shim.join("node");
        fs::write(
            &node_shim,
            "#!/bin/sh\nout=\"\"\nwhile [ $# -gt 0 ]; do\n  if [ \"$1\" = \"--out\" ]; then out=\"$2\"; shift 2; continue; fi\n  shift\ndone\nprintf '[{\"id_str\":\"1\",\"animate_url\":{\"url_list\":[\"https://p3-im-emoticon-sign.byteimg.com/a.webp\"]}},{\"id_str\":\"2\",\"static_url\":{\"url_list\":[\"https://p3-im-emoticon-sign.byteimg.com/b.webp\"]}}]' > \"$out\"\necho found 2 >&2\n",
        )
        .unwrap();
        let python_shim = shim.join("python3");
        fs::write(
            &python_shim,
            "#!/bin/sh\ndest=\"$3\"\nmkdir -p \"$dest/originals\"\npython3 - \"$dest\" <<'PY2' 2>/dev/null || true\nPY2\nexit 0\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&node_shim).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&node_shim, permissions.clone()).unwrap();
        fs::set_permissions(&python_shim, permissions).unwrap();
        // python shim needs to actually create originals; use printf-based png.
        fs::write(
            &python_shim,
            "#!/bin/sh\ndest=\"$3\"\nmkdir -p \"$dest/originals\"\ntouch \"$dest/report.json\"\nexit 0\n",
        )
        .unwrap();
        fs::set_permissions(&python_shim, fs::metadata(&node_shim).unwrap().permissions()).unwrap();
        let old_path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{shim}:{old_path}", shim = shim.display()));
        let scripts = dir.path().join("scripts");
        fs::create_dir(&scripts).unwrap();
        fs::write(scripts.join("collect_douyin.mjs"), "// shim\n").unwrap();
        fs::write(scripts.join("download_douyin.py"), "# shim\n").unwrap();
        let result = lib.collect_douyin_fetch(&scripts, 9222);
        std::env::set_var("PATH", old_path);
        let result = result.unwrap();
        assert_eq!(result.stage, "fetch");
        assert!(result.detail.contains("2 项"));
        assert!(root.join("downloads/stickers.json").exists());
    }
    #[test]
    fn wechat_manifest_downloads_and_imports_with_account_mapping() {
        let _path_guard = PATH_LOCK.lock().unwrap();
        // Shim python3 that decodes nothing but stages two tiny PNG originals.
        use std::os::unix::fs::PermissionsExt;
        let (dir, lib, _ids) = fixture();
        let root = PathBuf::from(lib.snapshot().root);
        let shim = dir.path().join("shim");
        fs::create_dir(&shim).unwrap();
        let downloader = shim.join("python3");
        let shim_script = format!(
            "#!/bin/sh\ndest=\"$3\"\nmkdir -p \"$dest/originals\"\ncp '{src}/a.png' \"$dest/originals/$(/usr/bin/shasum -a 256 '{src}/a.png' | cut -c1-64).png\"\ncp '{src}/b.png' \"$dest/originals/$(/usr/bin/shasum -a 256 '{src}/b.png' | cut -c1-64).png\"\ncp '{src}/report.json' \"$dest/report.json\"\nprintf '{{\"total\":2,\"verified\":2,\"failed\":0,\"skipped\":0}}'\n\nexit 0\n",
            src = shim.display()
        );
        fs::write(&downloader, shim_script)
        .unwrap();
        let mut permissions = fs::metadata(&downloader).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&downloader, permissions).unwrap();
        // Stage fixture assets for the shim to copy.
        image::RgbaImage::from_pixel(2, 2, image::Rgba([9, 8, 7, 255]))
            .save(shim.join("a.png"))
            .unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]))
            .save(shim.join("b.png"))
            .unwrap();
        let shim_report = shim.join("report.json");
        let hash_a = {
            let data = fs::read(shim.join("a.png")).unwrap();
            format!("{:x}", sha2::Sha256::digest(&data))
        };
        fs::write(&shim_report, serde_json::to_vec(&serde_json::json!({
            "schema_version": 1, "expected_items": 2, "items": [
                {"id": "1", "status": "verified", "url": "https://mmbiz.qpic.cn/a", "sha256": hash_a, "format": "png", "resource_identity": "b".repeat(64)},
                {"id": "2", "status": "pending_inspection", "resource_identity": "c".repeat(64)}
            ]
        })).unwrap()).unwrap();
        let old_path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{}", shim.display(), old_path));
        let scripts = dir.path().join("scripts");
        fs::create_dir(&scripts).unwrap();
        fs::write(scripts.join("download_wechat.py"), "# shim\n").unwrap();
        let manifest = dir.path().join("emoticon_urls.txt");
        fs::write(&manifest, "https://mmbiz.qpic.cn/a
https://mmbiz.qpic.cn/b
").unwrap();
        let staged = lib.import_wechat_manifest(&scripts, &manifest);
        std::env::set_var("PATH", old_path);
        let staged = staged.unwrap();
        assert!(staged.detail.contains("下载成功 2"));
        // Import + map to a WeChat account.
        let mut lib = lib;
        let (report, report_path) = lib.collect_wechat_import().unwrap();
        // The staged report marks one item verified and one pending;
        // only the verified original enters the library.
        assert_eq!(report.added, 1);
        let accounts_before = lib.get_management().unwrap().accounts.len();
        let management = lib
            .import_provenance(&lib.snapshot().root, &report_path, "微信大号".into(), None, Some("微信".into()))
            .unwrap();
        assert_eq!(management.accounts.len(), accounts_before + 1);
        assert_eq!(management.accounts.last().unwrap().platform, "微信");
        assert_eq!(management.references.len(), 1);
        assert!(root.join("downloads/wechat/originals").exists());
    }
    #[test]
    fn wechat_manifest_partial_failure_still_proceeds() {
        let _path_guard = PATH_LOCK.lock().unwrap();
        use std::os::unix::fs::PermissionsExt;
        let (dir, lib, _ids) = fixture();
        let shim = dir.path().join("shim");
        fs::create_dir(&shim).unwrap();
        let downloader = shim.join("python3");
        let shim_script = format!(
            "#!/bin/sh\ndest=\"$3\"\nmkdir -p \"$dest/originals\"\ncp '{src}/a.png' \"$dest/originals/a.png\"\nprintf '{{\"total\":3,\"verified\":1,\"failed\":2,\"skipped\":0}}'\necho 'failed items' >&2\nexit 2\n",
            src = shim.display()
        );
        fs::write(&downloader, shim_script).unwrap();
        let mut permissions = fs::metadata(&downloader).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&downloader, permissions).unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 1, 1, 255]))
            .save(shim.join("a.png"))
            .unwrap();
        let old_path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{}", shim.display(), old_path));
        let scripts = dir.path().join("scripts");
        fs::create_dir(&scripts).unwrap();
        fs::write(scripts.join("download_wechat.py"), "# shim\n").unwrap();
        let manifest = dir.path().join("urls.txt");
        fs::write(&manifest, "https://mmbiz.qpic.cn/a
").unwrap();
        let staged = lib.import_wechat_manifest(&scripts, &manifest);
        std::env::set_var("PATH", old_path);
        let staged = staged.unwrap();
        assert!(staged.detail.contains("下载成功 1 · 失败 2"));
    }
    #[test]
    fn collect_wechat_import_uses_only_report_verified_files() {
        let (dir, lib, _ids) = fixture();
        let root = PathBuf::from(lib.snapshot().root);
        let originals = root.join("downloads").join("wechat").join("originals");
        fs::create_dir_all(&originals).unwrap();
        let ok = originals.join("ok.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([5, 5, 5, 255]))
            .save(&ok)
            .unwrap();
        // A stray file that is NOT referenced by the report must be ignored.
        image::RgbaImage::from_pixel(2, 2, image::Rgba([6, 6, 6, 255]))
            .save(originals.join("stray.png"))
            .unwrap();
        let hash = {
            let data = fs::read(&ok).unwrap();
            format!("{:x}", sha2::Sha256::digest(&data))
        };
        // The downloader names originals by content hash.
        fs::rename(&ok, originals.join(format!("{hash}.png"))).unwrap();
        fs::write(root.join("downloads").join("wechat").join("report.json"), serde_json::to_vec(&serde_json::json!({
            "schema_version": 1, "expected_items": 1, "items": [
                {"id": "1", "status": "verified", "url": "https://mmbiz.qpic.cn/a", "sha256": hash, "format": "png", "resource_identity": "b".repeat(64)}
            ]
        })).unwrap()).unwrap();
        let mut lib = lib;
        let (report, _path) = lib.collect_wechat_import().unwrap();
        assert_eq!(report.added, 1, "only the referenced file is imported");
        assert!(lib.import_files(vec![originals.join("stray.png")], "微信".into()).is_ok());
    }

    #[test]
    fn wechat_manifest_requires_existing_file_and_script() {
        let (dir, lib, _ids) = fixture();
        let scripts = dir.path().join("scripts");
        fs::create_dir(&scripts).unwrap();
        assert!(lib.import_wechat_manifest(&scripts, &dir.path().join("missing.txt")).is_err());
        fs::write(dir.path().join("urls.txt"), "https://mmbiz.qpic.cn/a
").unwrap();
        assert!(lib
            .import_wechat_manifest(&scripts, &dir.path().join("urls.txt"))
            .is_err());
    }

    #[test]
    fn export_sanitizes_names_and_rejects_empty_selection() {
        let (dir, lib, ids) = fixture();
        let names = std::collections::HashMap::from([(
            ids[0].clone(),
            "a/b\\c:d*e?f\"g|h😀".to_string(),
        )]);
        let summary = lib
            .export_assets(dir.path(), &[ids[0].clone()], &names)
            .unwrap();
        let target = std::path::PathBuf::from(summary.path);
        let name = std::fs::read_dir(&target)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .file_name();
        let name = name.to_string_lossy();
        assert!(!name.contains('/') && !name.contains('\\') && !name.contains(':'));
        assert!(name.contains('😀'));
        assert!(name.ends_with(".png"));
        assert!(lib
            .export_assets(dir.path(), &[], &Default::default())
            .is_err());
    }
    #[test]
    fn export_strips_spoofing_characters_and_truncates_by_bytes() {
        let (dir, lib, ids) = fixture();
        // Bidi override + zero-width + BOM: all must be stripped.
        let sneaky = "ok\u{202E}gpj\u{200B}.\u{FEFF}png".to_string();
        let names = std::collections::HashMap::from([(ids[0].clone(), sneaky)]);
        let summary = lib
            .export_assets(dir.path(), &[ids[0].clone()], &names)
            .unwrap();
        let target = std::path::PathBuf::from(&summary.path);
        let entry = std::fs::read_dir(&target).unwrap().next().unwrap().unwrap();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        assert!(!name.chars().any(|c| ('\u{2000}'..='\u{206F}').contains(&c) || c == '\u{FEFF}'));
        // 100 four-byte emoji must truncate within the byte budget and never error.
        let long = "😀".repeat(100);
        let names = std::collections::HashMap::from([(ids[1].clone(), long)]);
        let summary = lib
            .export_assets(dir.path(), &[ids[1].clone()], &names)
            .unwrap();
        let entry = std::fs::read_dir(std::path::PathBuf::from(&summary.path))
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        let name = entry.file_name();
        assert!(name.to_string_lossy().len() <= 255);
        assert!(name.to_string_lossy().ends_with(".png"));
    }
}

//! Local annotations and provenance. All access is through the already locked Library.
use crate::library::Library;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
const LIMIT: usize = 20 * 1024 * 1024;
const MAX_RECORDS: usize = 20_000;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetMetadata {
    pub name: String,
    pub tags: Vec<String>,
    pub collections: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Account {
    pub id: String,
    pub platform: String,
    pub alias: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reference {
    pub account_id: String,
    pub sticker_id: String,
    pub asset_id: String,
    pub resource_identity: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Batch {
    pub id: String,
    pub account_id: String,
    pub collection_items: usize,
    pub mapped_resources: usize,
    pub failed_resources: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagementSnapshot {
    pub version: u32,
    pub metadata: BTreeMap<String, AssetMetadata>,
    pub accounts: Vec<Account>,
    pub references: Vec<Reference>,
    pub batches: Vec<Batch>,
}
impl Default for ManagementSnapshot {
    fn default() -> Self {
        Self {
            version: 1,
            metadata: BTreeMap::new(),
            accounts: vec![],
            references: vec![],
            batches: vec![],
        }
    }
}
#[derive(Deserialize)]
struct Report {
    schema_version: u32,
    expected_items: usize,
    items: Vec<ReportItem>,
}
#[derive(Deserialize)]
struct ReportItem {
    id: String,
    status: String,
    sha256: Option<String>,
    resource_identity: Option<String>,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash_valid(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn text_valid(s: &str, max: usize, empty: bool) -> bool {
    (empty || !s.is_empty())
        && s.chars().count() <= max
        && s.trim() == s
        && !s.chars().any(char::is_control)
}
fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    if !fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_file()
    {
        return Err("必须使用普通文件，不能使用符号链接".into());
    }
    let mut data = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > LIMIT {
        return Err("管理文件或采集报告超过 20 MiB".into());
    }
    Ok(data)
}
fn metadata_valid(m: &AssetMetadata) -> bool {
    text_valid(&m.name, 200, true)
        && [&m.tags, &m.collections].iter().all(|list| {
            list.len() <= 50
                && list.iter().all(|s| text_valid(s, 50, false))
                && list.iter().collect::<HashSet<_>>().len() == list.len()
        })
}
impl Library {
    fn management_path(&self) -> PathBuf {
        PathBuf::from(self.snapshot().root).join("management.json")
    }
    pub fn check_expected_root(&self, root: &str) -> Result<(), String> {
        if self.snapshot().root != root {
            Err("资料库已切换，请重新打开编辑窗口".into())
        } else {
            Ok(())
        }
    }
    fn validate_management(&self, m: &ManagementSnapshot) -> Result<(), String> {
        if m.version != 1
            || m.metadata.len() > MAX_RECORDS
            || m.references.len() > MAX_RECORDS
            || m.accounts.len() > 1000
            || m.batches.len() > MAX_RECORDS
        {
            return Err("管理文件版本或记录数量无效".into());
        }
        let mut accounts = HashSet::new();
        for a in &m.accounts {
            if !hash_valid(&a.id)
                || a.platform != "抖音"
                || !text_valid(&a.alias, 80, false)
                || !accounts.insert(a.id.as_str())
            {
                return Err("管理文件账号无效".into());
            }
        }
        for (id, meta) in &m.metadata {
            if !hash_valid(id) || !metadata_valid(meta) {
                return Err("管理文件素材信息无效".into());
            }
            self.asset_path(id)?;
        }
        let mut refs = HashSet::new();
        for r in &m.references {
            if !accounts.contains(r.account_id.as_str())
                || !text_valid(&r.sticker_id, 128, false)
                || !r.sticker_id.bytes().all(|b| b.is_ascii_digit())
                || !hash_valid(&r.asset_id)
                || !hash_valid(&r.resource_identity)
                || !refs.insert(r)
            {
                return Err("管理文件来源引用无效".into());
            }
            self.asset_path(&r.asset_id)?;
        }
        let mut batches = HashSet::new();
        for b in &m.batches {
            if !hash_valid(&b.id)
                || !accounts.contains(b.account_id.as_str())
                || !batches.insert((&b.id, &b.account_id))
                || b.collection_items > MAX_RECORDS
                || b.mapped_resources > b.collection_items
                || b.failed_resources > b.collection_items.saturating_sub(b.mapped_resources)
            {
                return Err("管理文件采集批次无效".into());
            }
        }
        Ok(())
    }
    pub fn get_management(&self) -> Result<ManagementSnapshot, String> {
        let path = self.management_path();
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ManagementSnapshot::default())
            }
            Err(e) => return Err(e.to_string()),
            Ok(_) => (),
        }
        let m: ManagementSnapshot = serde_json::from_slice(&read_file(&path)?)
            .map_err(|e| format!("管理文件损坏，未覆盖原文件：{e}"))?;
        self.validate_management(&m)?;
        Ok(m)
    }
    fn write_management(&self, m: &ManagementSnapshot) -> Result<(), String> {
        self.validate_management(m)?;
        let path = self.management_path();
        match fs::symlink_metadata(&path) {
            Ok(meta) if !meta.file_type().is_file() => return Err("管理文件不能是符号链接".into()),
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            _ => (),
        }
        let data = serde_json::to_vec_pretty(m).map_err(|e| e.to_string())?;
        if data.len() > LIMIT {
            return Err("管理文件超过 20 MiB".into());
        }
        let mut temp =
            tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(|e| e.to_string())?;
        temp.write_all(&data).map_err(|e| e.to_string())?;
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        temp.persist(path).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn save_metadata(
        &self,
        expected_root: &str,
        asset_id: String,
        name: String,
        tags: Vec<String>,
        collections: Vec<String>,
    ) -> Result<ManagementSnapshot, String> {
        self.check_expected_root(expected_root)?;
        self.asset_path(&asset_id)?;
        let mut m = self.get_management()?;
        let normalize = |list: Vec<String>| {
            let mut seen = HashSet::new();
            list.into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && seen.insert(s.clone()))
                .collect()
        };
        if tags.len() > 50 || collections.len() > 50 {
            return Err("标签或合集最多 50 个".into());
        }
        let meta = AssetMetadata {
            name: name.trim().into(),
            tags: normalize(tags),
            collections: normalize(collections),
        };
        if !metadata_valid(&meta) {
            return Err("名称最多 200 字，标签和合集最多 50 字，不能含控制字符".into());
        }
        if meta.name.is_empty() && meta.tags.is_empty() && meta.collections.is_empty() {
            m.metadata.remove(&asset_id);
        } else {
            m.metadata.insert(asset_id, meta);
        }
        self.write_management(&m)?;
        Ok(m)
    }
    pub fn import_provenance(
        &self,
        expected_root: &str,
        path: &Path,
        alias: String,
        account_id: Option<String>,
    ) -> Result<ManagementSnapshot, String> {
        self.check_expected_root(expected_root)?;
        let mut m = self.get_management()?;
        let bytes = read_file(path)?;
        let report: Report =
            serde_json::from_slice(&bytes).map_err(|e| format!("采集报告格式错误：{e}"))?;
        if report.schema_version != 1
            || report.expected_items > MAX_RECORDS
            || report.items.len() > report.expected_items
            || report.items.is_empty()
        {
            return Err("报告版本、数量或内容无效".into());
        }
        let alias = alias.trim().to_string();
        let account_id = if let Some(id) = account_id {
            if !m
                .accounts
                .iter()
                .any(|a| a.id == id && a.platform == "抖音")
            {
                return Err("所选账号不存在".into());
            }
            id
        } else {
            if !text_valid(&alias, 80, false) {
                return Err("请填写 1–80 字的本地账号别名".into());
            }
            if m.accounts.iter().any(|a| a.alias == alias) {
                return Err("别名已存在，请选择已有账号或使用其他别名".into());
            }
            let seed = format!(
                "{}:{}:{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos(),
                m.accounts.len(),
                alias
            );
            let id = digest(seed.as_bytes());
            m.accounts.push(Account {
                id: id.clone(),
                platform: "抖音".into(),
                alias,
            });
            id
        };
        let mut ids = HashSet::new();
        let mut mapped = 0;
        let mut failed = 0;
        let mut existing: HashSet<_> = m.references.iter().cloned().collect();
        for row in report.items {
            if !text_valid(&row.id, 128, false)
                || !row.id.bytes().all(|b| b.is_ascii_digit())
                || !ids.insert(row.id.clone())
            {
                return Err("报告含重复或无效收藏 ID".into());
            }
            if row.status == "failed" {
                failed += 1;
                continue;
            }
            if row.status != "verified" {
                return Err("报告含未完成校验的素材，请先完成入库".into());
            }
            let asset_id = row.sha256.ok_or("报告缺少素材哈希")?;
            let resource_identity = row.resource_identity.ok_or("报告缺少资源身份摘要")?;
            if !hash_valid(&asset_id) || !hash_valid(&resource_identity) {
                return Err("报告哈希无效".into());
            }
            self.asset_path(&asset_id)
                .map_err(|_| "报告引用素材尚未入库或已缺失；本次来源映射未写入".to_string())?;
            let r = Reference {
                account_id: account_id.clone(),
                sticker_id: row.id,
                asset_id,
                resource_identity,
            };
            if existing.insert(r.clone()) {
                m.references.push(r);
            }
            mapped += 1;
        }
        if mapped == 0 {
            return Err("报告没有可映射的已验证素材".into());
        }
        let batch_id = digest(&bytes);
        // Keep unique batches, ordered by their most recent import observation.
        if let Some(index) = m
            .batches
            .iter()
            .position(|b| b.id == batch_id && b.account_id == account_id)
        {
            m.batches.remove(index);
        }
        m.batches.push(Batch {
            id: batch_id,
            account_id,
            collection_items: report.expected_items,
            mapped_resources: mapped,
            failed_resources: failed,
        });
        self.write_management(&m)?;
        Ok(m)
    }
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
    fn report(path: &Path, id: &str) {
        fs::write(
            path,
            serde_json::to_vec(
                &serde_json::json!({"schema_version":1,"expected_items":2,"items":[
                    {"id":"100","status":"verified","sha256":id,"resource_identity":"a".repeat(64)},
                    {"id":"200","status":"verified","sha256":id,"resource_identity":"a".repeat(64)}
                ]}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    #[test]
    fn annotations_do_not_change_assets_or_manifest() {
        let (_dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        let manifest = fs::read(Path::new(&root).join("library.json")).unwrap();
        let bytes = fs::read(lib.asset_path(&ids[0]).unwrap()).unwrap();
        let m = lib
            .save_metadata(
                &root,
                ids[0].clone(),
                " 新名称 ".into(),
                vec!["好玩".into(), "好玩".into()],
                vec!["自用".into()],
            )
            .unwrap();
        assert_eq!(m.metadata[&ids[0]].tags, vec!["好玩"]);
        assert_eq!(
            manifest,
            fs::read(Path::new(&root).join("library.json")).unwrap()
        );
        assert_eq!(bytes, fs::read(lib.asset_path(&ids[0]).unwrap()).unwrap());
        assert!(lib
            .save_metadata("wrong", ids[0].clone(), "x".into(), vec![], vec![])
            .is_err());
        let m = lib
            .save_metadata(&root, ids[0].clone(), "".into(), vec![], vec![])
            .unwrap();
        assert!(m.metadata.is_empty());
    }
    #[test]
    fn annotations_persist_after_library_reopen() {
        let (_dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        lib.save_metadata(
            &root,
            ids[0].clone(),
            "我的表情".into(),
            vec!["开心".into()],
            vec!["常用".into()],
        )
        .unwrap();
        drop(lib);
        let reopened = Library::open(Path::new(&root)).unwrap();
        let management = reopened.get_management().unwrap();
        let metadata = &management.metadata[&ids[0]];
        assert_eq!(metadata.name, "我的表情");
        assert_eq!(metadata.tags, vec!["开心"]);
        assert_eq!(metadata.collections, vec!["常用"]);
    }
    #[test]
    fn partial_report_keeps_expected_mapped_and_failed_counts_separate() {
        let (dir, lib, ids) = fixture();
        let path = dir.path().join("partial-report.json");
        fs::write(&path, serde_json::to_vec(&serde_json::json!({
            "schema_version": 1, "expected_items": 3, "items": [
                {"id": "100", "status": "verified", "sha256": ids[0], "resource_identity": "a".repeat(64)},
                {"id": "200", "status": "failed", "error": "download_failed"}
            ]
        })).unwrap()).unwrap();
        let management = lib
            .import_provenance(&lib.snapshot().root, &path, "部分采集测试".into(), None)
            .unwrap();
        assert_eq!(management.references.len(), 1);
        assert_eq!(management.batches.len(), 1);
        let batch = &management.batches[0];
        assert_eq!(batch.collection_items, 3);
        assert_eq!(batch.mapped_resources, 1);
        assert_eq!(batch.failed_resources, 1);
        assert_eq!(
            batch.collection_items - batch.mapped_resources - batch.failed_resources,
            1
        );
    }
    #[test]
    fn corrupt_management_is_not_overwritten() {
        let (_dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        let p = lib.management_path();
        fs::write(&p, b"broken").unwrap();
        assert!(lib.get_management().is_err());
        assert!(lib
            .save_metadata(&root, ids[0].clone(), "x".into(), vec![], vec![])
            .is_err());
        assert_eq!(fs::read(p).unwrap(), b"broken");
    }
    #[test]
    fn provenance_is_idempotent_and_keeps_versions() {
        let (dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        let p = dir.path().join("report.json");
        report(&p, &ids[0]);
        let m = lib
            .import_provenance(&root, &p, "测试账号".into(), None)
            .unwrap();
        assert_eq!(m.references.len(), 2);
        let account = m.accounts[0].id.clone();
        assert!(lib
            .import_provenance(&root, &p, "测试账号".into(), None)
            .is_err());
        let m = lib
            .import_provenance(&root, &p, "".into(), Some(account.clone()))
            .unwrap();
        assert_eq!(m.references.len(), 2);
        assert_eq!(m.batches.len(), 1);
        report(&p, &ids[1]);
        let m = lib
            .import_provenance(&root, &p, "".into(), Some(account))
            .unwrap();
        assert_eq!(m.references.len(), 4);
        assert_eq!(m.batches.len(), 2);
    }
    #[test]
    fn reimported_partial_batch_is_the_latest_without_duplicate_references() {
        let (dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        let partial = dir.path().join("partial.json");
        fs::write(&partial, serde_json::to_vec(&serde_json::json!({
            "schema_version": 1, "expected_items": 3, "items": [
                {"id": "100", "status": "verified", "sha256": ids[0], "resource_identity": "a".repeat(64)},
                {"id": "200", "status": "failed"}
            ]
        })).unwrap()).unwrap();
        let first = lib
            .import_provenance(&root, &partial, "测试".into(), None)
            .unwrap();
        let account = first.accounts[0].id.clone();
        let batch_id = first.batches[0].id.clone();
        let complete = dir.path().join("complete.json");
        report(&complete, &ids[0]);
        let second = lib
            .import_provenance(&root, &complete, "".into(), Some(account.clone()))
            .unwrap();
        assert_eq!(second.batches.last().unwrap().failed_resources, 0);
        let repeated = lib
            .import_provenance(&root, &partial, "".into(), Some(account))
            .unwrap();
        assert_eq!(repeated.references.len(), second.references.len());
        assert_eq!(repeated.batches.len(), 2);
        let latest = repeated.batches.last().unwrap();
        assert_eq!(latest.id, batch_id);
        assert_eq!(latest.collection_items, 3);
        assert_eq!(latest.mapped_resources, 1);
        assert_eq!(latest.failed_resources, 1);
    }
    #[test]
    fn missing_asset_rejects_entire_mapping() {
        let (dir, lib, _) = fixture();
        let p = dir.path().join("report.json");
        report(&p, &"e".repeat(64));
        assert!(lib
            .import_provenance(&lib.snapshot().root, &p, "测试".into(), None)
            .is_err());
        assert!(!lib.management_path().exists());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_management_and_report_are_rejected() {
        use std::os::unix::fs::symlink;
        let (dir, lib, ids) = fixture();
        let p = dir.path().join("report.json");
        report(&p, &ids[0]);
        symlink(&p, lib.management_path()).unwrap();
        assert!(lib.get_management().is_err());
        fs::remove_file(lib.management_path()).unwrap();
        let linked = dir.path().join("linked.json");
        symlink(&p, &linked).unwrap();
        assert!(lib
            .import_provenance(&lib.snapshot().root, &linked, "测试".into(), None)
            .is_err());
    }
}

//! Local annotations and provenance. All access is through the already locked Library.
use crate::library::Library;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
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
pub struct VersionGroup {
    pub id: String,
    pub main_asset_id: String,
    pub member_ids: Vec<String>,
    pub tags: Vec<String>,
    pub collections: Vec<String>,
    pub created_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagementSnapshot {
    pub version: u32,
    pub metadata: BTreeMap<String, AssetMetadata>,
    pub accounts: Vec<Account>,
    pub references: Vec<Reference>,
    pub batches: Vec<Batch>,
    /// Logical recycle bin: asset id → trashed-at unix seconds. Files stay in assets/.
    #[serde(default)]
    pub trash: BTreeMap<String, u64>,
    /// Manual duplicate review groups; originals stay untouched.
    #[serde(default)]
    pub groups: Vec<VersionGroup>,
    /// Similarity pairs the user chose to ignore; never re-suggested.
    #[serde(default)]
    pub ignored_pairs: Vec<(String, String)>,
}
impl Default for ManagementSnapshot {
    fn default() -> Self {
        Self {
            version: 1,
            metadata: BTreeMap::new(),
            accounts: vec![],
            references: vec![],
            batches: vec![],
            trash: BTreeMap::new(),
            groups: vec![],
            ignored_pairs: vec![],
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
        && !s.chars().any(|c| {
            c.is_control()
                || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}')
        })
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
            || m.trash.len() > MAX_RECORDS
            || m.groups.len() > MAX_RECORDS
            || m.ignored_pairs.len() > MAX_RECORDS
        {
            return Err("管理文件版本或记录数量无效".into());
        }
        let mut accounts = HashSet::new();
        for a in &m.accounts {
            if !hash_valid(&a.id)
                || !matches!(a.platform.as_str(), "抖音" | "微信")
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
        for (id, at) in &m.trash {
            if !hash_valid(id) || *at == 0 {
                return Err("管理文件回收站记录无效".into());
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
        let mut grouped = HashSet::new();
        let mut group_ids = HashSet::new();
        for g in &m.groups {
            if !hash_valid(&g.id)
                || !hash_valid(&g.main_asset_id)
                || g.member_ids.len() < 2
                || g.member_ids.len() > 500
                || g.created_at == 0
                || !g.member_ids.contains(&g.main_asset_id)
                || !group_ids.insert(&g.id)
                || g.tags.len() > 50
                || g.collections.len() > 50
                || !g.tags.iter().all(|s| text_valid(s, 50, false))
                || !g.collections.iter().all(|s| text_valid(s, 50, false))
            {
                return Err("管理文件版本分组无效".into());
            }
            self.asset_path(&g.main_asset_id)?;
            for member in &g.member_ids {
                if !hash_valid(member) || !grouped.insert(member) {
                    return Err("管理文件版本分组无效".into());
                }
                self.asset_path(member)?;
            }
        }
        for (a, b) in &m.ignored_pairs {
            if a >= b || !hash_valid(a) || !hash_valid(b) {
                return Err("管理文件忽略对比无效".into());
            }
            self.asset_path(a)?;
            self.asset_path(b)?;
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
    /// Batch rename with a prefix and a running number, in the given order.
    /// Only visible (non-trashed) assets can be renamed; display names only.
    pub fn batch_rename(
        &self,
        expected_root: &str,
        asset_ids: Vec<String>,
        prefix: String,
        start: u32,
    ) -> Result<ManagementSnapshot, String> {
        self.check_expected_root(expected_root)?;
        if asset_ids.is_empty() || asset_ids.len() > 2000 {
            return Err("一次最多批量重命名 2000 个素材".into());
        }
        let prefix = prefix.trim().to_string();
        if !text_valid(&prefix, 190, false) {
            return Err("前缀为 1–190 字，不能含控制字符".into());
        }
        if start == 0 || start + asset_ids.len() as u32 - 1 > 999_999 {
            return Err("起始序号需为 1–999999，且不超过范围".into());
        }
        let mut m = self.get_management()?;
        let mut seen = HashSet::new();
        for (index, id) in asset_ids.iter().enumerate() {
            if !hash_valid(id) || !seen.insert(id) {
                return Err("素材标识无效".into());
            }
            self.asset_path(id)?;
            if m.trash.contains_key(id) {
                return Err("回收站中的素材不能批量重命名".into());
            }
            let name = format!("{prefix}-{:03}", start + index as u32);
            let mut meta = m.metadata.get(id).cloned().unwrap_or_default();
            meta.name = name;
            m.metadata.insert(id.clone(), meta);
        }
        self.write_management(&m)?;
        Ok(m)
    }
    /// Add and/or remove tags and collections in batch, keeping per-asset names.
    pub fn batch_labels(
        &self,
        expected_root: &str,
        asset_ids: Vec<String>,
        add_tags: Vec<String>,
        remove_tags: Vec<String>,
        add_collections: Vec<String>,
        remove_collections: Vec<String>,
    ) -> Result<ManagementSnapshot, String> {
        self.check_expected_root(expected_root)?;
        if asset_ids.is_empty() || asset_ids.len() > 2000 {
            return Err("一次最多批量处理 2000 个素材".into());
        }
        let normalize = |list: Vec<String>, field: &str| -> Result<Vec<String>, String> {
            let mut seen = HashSet::new();
            let values: Vec<String> = list
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && seen.insert(s.clone()))
                .collect();
            if values.len() > 50 || !values.iter().all(|v| text_valid(v, 50, false)) {
                return Err(format!("{field}最多 50 个、每项 50 字，不能含控制字符"));
            }
            Ok(values)
        };
        let add_tags = normalize(add_tags, "标签")?;
        let remove_tags = normalize(remove_tags, "标签")?;
        let add_collections = normalize(add_collections, "合集")?;
        let remove_collections = normalize(remove_collections, "合集")?;
        if add_tags.is_empty() && remove_tags.is_empty()
            && add_collections.is_empty() && remove_collections.is_empty()
        {
            return Err("请至少填写一个要添加或移除的标签或合集".into());
        }
        let mut m = self.get_management()?;
        let limit_lists = |meta: &mut AssetMetadata| -> Result<(), String> {
            if meta.tags.len() > 50 || meta.collections.len() > 50 {
                return Err("单个素材的标签或合集最多 50 个".into());
            }
            Ok(())
        };
        let mut seen = HashSet::new();
        for id in &asset_ids {
            if !hash_valid(id) || !seen.insert(id) {
                return Err("素材标识无效".into());
            }
            self.asset_path(id)?;
            if m.trash.contains_key(id) {
                return Err("回收站中的素材不能批量整理".into());
            }
            let mut meta = m.metadata.get(id).cloned().unwrap_or_default();
            for tag in &add_tags {
                if !meta.tags.contains(tag) {
                    meta.tags.push(tag.clone());
                }
            }
            meta.tags.retain(|t| !remove_tags.contains(t));
            for collection in &add_collections {
                if !meta.collections.contains(collection) {
                    meta.collections.push(collection.clone());
                }
            }
            meta.collections.retain(|c| !remove_collections.contains(c));
            limit_lists(&mut meta)?;
            if meta.name.is_empty() && meta.tags.is_empty() && meta.collections.is_empty() {
                m.metadata.remove(id);
            } else {
                m.metadata.insert(id.clone(), meta);
            }
        }
        self.write_management(&m)?;
        Ok(m)
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
    /// Logical trash only: asset files and library.json stay untouched.
    pub fn set_trash(
        &self,
        expected_root: &str,
        asset_ids: Vec<String>,
        trashed: bool,
    ) -> Result<ManagementSnapshot, String> {
        self.check_expected_root(expected_root)?;
        if asset_ids.is_empty() || asset_ids.len() > 2000 {
            return Err("一次最多处理 2000 个素材".into());
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
        let mut m = self.get_management()?;
        let mut seen = HashSet::new();
        for id in asset_ids {
            if !hash_valid(&id) {
                return Err("素材标识无效".into());
            }
            self.asset_path(&id)?;
            if !seen.insert(id.clone()) {
                continue;
            }
            if trashed && m.groups.iter().any(|g| g.member_ids.contains(&id)) {
                // Guard both directions: disbanding needs every member visible.
                return Err("版本分组中的素材不能移入回收站，请先拆分分组".into());
            }
            if trashed {
                // Keep the original trash time when the item is already trashed.
                m.trash.entry(id).or_insert(now);
            } else {
                m.trash.remove(&id);
            }
        }
        self.write_management(&m)?;
        Ok(m)
    }
    /// Group assets for manual duplicate review. Requires at least two members,
    /// all present, none trashed, none already grouped; originals stay untouched.
    pub fn create_group(
        &self,
        expected_root: &str,
        member_ids: Vec<String>,
        main_asset_id: String,
        tags: Vec<String>,
        collections: Vec<String>,
    ) -> Result<ManagementSnapshot, String> {
        self.check_expected_root(expected_root)?;
        if member_ids.len() < 2 || member_ids.len() > 500 {
            return Err("版本分组需要 2–500 个素材".into());
        }
        let mut m = self.get_management()?;
        let mut seen = HashSet::new();
        for member in &member_ids {
            if !hash_valid(member) || !seen.insert(member) {
                return Err("版本分组成员无效".into());
            }
            self.asset_path(member)?;
            if m.trash.contains_key(member) {
                return Err("回收站中的素材不能加入版本分组".into());
            }
            if m.groups.iter().any(|g| g.member_ids.contains(member)) {
                return Err("素材已在其他版本分组中".into());
            }
        }
        if !seen.contains(&main_asset_id) {
            return Err("主展示版本必须是分组成员".into());
        }
        let normalize = |list: Vec<String>| {
            let mut seen = HashSet::new();
            list.into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && seen.insert(s.clone()))
                .collect::<Vec<_>>()
        };
        let tags = normalize(tags);
        let collections = normalize(collections);
        if tags.len() > 50 || collections.len() > 50 {
            return Err("分组的标签或合集最多 50 个".into());
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?;
        let seed = format!("{:?}:{}", member_ids, now.as_nanos());
        m.groups.push(VersionGroup {
            id: digest(seed.as_bytes()),
            main_asset_id,
            member_ids,
            tags,
            collections,
            created_at: now.as_secs(),
        });
        self.write_management(&m)?;
        Ok(m)
    }
    /// Edit a group's own tags/collections (group-level metadata, shown as a
    /// union with member metadata). Members keep their own values.
    pub fn save_group_metadata(
        &self,
        expected_root: &str,
        group_id: &str,
        tags: Vec<String>,
        collections: Vec<String>,
    ) -> Result<ManagementSnapshot, String> {
        self.check_expected_root(expected_root)?;
        let mut m = self.get_management()?;
        let group = m
            .groups
            .iter_mut()
            .find(|g| g.id == group_id)
            .ok_or("版本分组不存在")?;
        let normalize = |list: Vec<String>| {
            let mut seen = HashSet::new();
            list.into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && seen.insert(s.clone()))
                .collect::<Vec<_>>()
        };
        let tags = normalize(tags);
        let collections = normalize(collections);
        if tags.len() > 50 || collections.len() > 50 {
            return Err("分组的标签或合集最多 50 个".into());
        }
        if !tags.iter().all(|s| text_valid(s, 50, false))
            || !collections.iter().all(|s| text_valid(s, 50, false))
        {
            return Err("标签和合集最多 50 字，不能含控制字符".into());
        }
        group.tags = tags;
        group.collections = collections;
        self.write_management(&m)?;
        Ok(m)
    }
    /// Remove a group. Members keep their own metadata; assets are untouched.
    pub fn disband_group(
        &self,
        expected_root: &str,
        group_id: &str,
    ) -> Result<ManagementSnapshot, String> {
        self.check_expected_root(expected_root)?;
        let mut m = self.get_management()?;
        let before = m.groups.len();
        m.groups.retain(|g| g.id != group_id);
        if m.groups.len() == before {
            return Err("版本分组不存在".into());
        }
        self.write_management(&m)?;
        Ok(m)
    }
    /// Never suggest this similarity pair again. The pair is stored ordered.
    pub fn ignore_pair(
        &self,
        expected_root: &str,
        asset_a: &str,
        asset_b: &str,
    ) -> Result<ManagementSnapshot, String> {
        self.check_expected_root(expected_root)?;
        if asset_a == asset_b || !hash_valid(asset_a) || !hash_valid(asset_b) {
            return Err("忽略对无效".into());
        }
        self.asset_path(asset_a)?;
        self.asset_path(asset_b)?;
        let mut m = self.get_management()?;
        let pair = if asset_a < asset_b {
            (asset_a.to_string(), asset_b.to_string())
        } else {
            (asset_b.to_string(), asset_a.to_string())
        };
        if !m.ignored_pairs.contains(&pair) {
            m.ignored_pairs.push(pair);
            m.ignored_pairs.sort();
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
        platform: Option<String>,
    ) -> Result<ManagementSnapshot, String> {
        self.check_expected_root(expected_root)?;
        let platform = match platform.as_deref() {
            None | Some("抖音") => "抖音",
            Some("微信") => "微信",
            _ => return Err("平台必须为抖音或微信".into()),
        };
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
                .any(|a| a.id == id && a.platform == platform)
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
                platform: platform.to_string(),
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
            if row.status == "failed" || row.status == "pending_inspection" {
                // Failed downloads and undecodable staged files both stay out
                // of the mapping; they remain visible in the batch counts.
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
            .import_provenance(&lib.snapshot().root, &path, "部分采集测试".into(), None, None)
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
    fn wechat_platform_accounts_are_separate_from_douyin() {
        let (dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        let p = dir.path().join("report.json");
        report(&p, &ids[0]);
        // A WeChat account is created with the same report, stored separately.
        let m = lib
            .import_provenance(&root, &p, "微信大号".into(), None, Some("微信".into()))
            .unwrap();
        assert_eq!(m.accounts.len(), 1);
        assert_eq!(m.accounts[0].platform, "微信");
        // Douyin account must not be confused with WeChat alias.
        let m = lib
            .import_provenance(&root, &p, "测试账号".into(), None, None)
            .unwrap();
        assert_eq!(m.accounts.len(), 2);
        // Reload persists both platforms.
        drop(lib);
        let reopened = Library::open(Path::new(&root)).unwrap();
        let m = reopened.get_management().unwrap();
        assert_eq!(m.accounts.len(), 2);
        assert!(m.accounts.iter().any(|a| a.platform == "微信"));
        assert!(reopened
            .import_provenance(&root, &p, "X".into(), None, Some("qq".into()))
            .is_err());
        let _ = dir;
    }
    #[test]
    fn pending_inspection_rows_count_as_failures_not_errors() {
        let (dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        let path = dir.path().join("report.json");
        fs::write(&path, serde_json::to_vec(&serde_json::json!({
            "schema_version": 1, "expected_items": 3, "items": [
                {"id": "100", "status": "verified", "sha256": ids[0], "resource_identity": "a".repeat(64)},
                {"id": "200", "status": "pending_inspection", "error": "unidentified_image"},
                {"id": "300", "status": "failed"}
            ]
        })).unwrap()).unwrap();
        let m = lib
            .import_provenance(&root, &path, "测试账号".into(), None, None)
            .unwrap();
        assert_eq!(m.references.len(), 1);
        let batch = &m.batches[0];
        assert_eq!((batch.collection_items, batch.mapped_resources, batch.failed_resources), (3, 1, 2));
    }
    #[test]
    fn provenance_is_idempotent_and_keeps_versions() {
        let (dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        let p = dir.path().join("report.json");
        report(&p, &ids[0]);
        let m = lib
            .import_provenance(&root, &p, "测试账号".into(), None, None)
            .unwrap();
        assert_eq!(m.references.len(), 2);
        let account = m.accounts[0].id.clone();
        assert!(lib
            .import_provenance(&root, &p, "测试账号".into(), None, None)
            .is_err());
        let m = lib
            .import_provenance(&root, &p, "".into(), Some(account.clone()), None)
            .unwrap();
        assert_eq!(m.references.len(), 2);
        assert_eq!(m.batches.len(), 1);
        report(&p, &ids[1]);
        let m = lib
            .import_provenance(&root, &p, "".into(), Some(account), None)
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
            .import_provenance(&root, &partial, "测试".into(), None, None)
            .unwrap();
        let account = first.accounts[0].id.clone();
        let batch_id = first.batches[0].id.clone();
        let complete = dir.path().join("complete.json");
        report(&complete, &ids[0]);
        let second = lib
            .import_provenance(&root, &complete, "".into(), Some(account.clone()), None)
            .unwrap();
        assert_eq!(second.batches.last().unwrap().failed_resources, 0);
        let repeated = lib
            .import_provenance(&root, &partial, "".into(), Some(account), None)
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
            .import_provenance(&lib.snapshot().root, &p, "测试".into(), None, None)
            .is_err());
        assert!(!lib.management_path().exists());
    }
    #[test]
    fn trash_and_restore_persist_without_touching_assets() {
        let (_dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        let manifest = fs::read(Path::new(&root).join("library.json")).unwrap();
        let bytes = fs::read(lib.asset_path(&ids[0]).unwrap()).unwrap();
        let m = lib
            .set_trash(&root, vec![ids[0].clone(), ids[0].clone()], true)
            .unwrap();
        assert_eq!(m.trash.len(), 1);
        assert!(m.trash[&ids[0]] > 0);
        assert_eq!(
            manifest,
            fs::read(Path::new(&root).join("library.json")).unwrap()
        );
        assert_eq!(bytes, fs::read(lib.asset_path(&ids[0]).unwrap()).unwrap());
        let m = lib.set_trash(&root, vec![ids[0].clone()], true).unwrap();
        assert_eq!(m.trash.len(), 1);
        drop(lib);
        let reopened = Library::open(Path::new(&root)).unwrap();
        let m = reopened.get_management().unwrap();
        assert_eq!(m.trash.len(), 1);
        let m = reopened
            .set_trash(&root, vec![ids[0].clone()], false)
            .unwrap();
        assert!(m.trash.is_empty());
        let m = reopened.set_trash(&root, vec![ids[0].clone()], false).unwrap();
        assert!(m.trash.is_empty());
    }
    #[test]
    fn trash_rejects_unknown_assets_wrong_root_and_empty_selection() {
        let (_dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        assert!(lib.set_trash("wrong", vec![ids[0].clone()], true).is_err());
        assert!(lib.set_trash(&root, vec![], true).is_err());
        assert!(lib.set_trash(&root, vec!["e".repeat(64)], true).is_err());
        assert!(lib.set_trash(&root, vec!["not-a-hash".into()], true).is_err());
        assert!(!lib.management_path().exists());
    }
    #[test]
    fn provenance_import_keeps_trashed_assets_trashed() {
        let (dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        lib.set_trash(&root, vec![ids[0].clone()], true).unwrap();
        let p = dir.path().join("report.json");
        report(&p, &ids[0]);
        let m = lib
            .import_provenance(&root, &p, "测试账号".into(), None, None)
            .unwrap();
        assert_eq!(m.references.len(), 2);
        assert_eq!(m.trash.len(), 1);
        assert!(m.trash.contains_key(&ids[0]));
    }
    #[test]
    fn groups_validate_membership_and_persist_without_touching_assets() {
        let (dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        let manifest = fs::read(Path::new(&root).join("library.json")).unwrap();
        // Too few members rejected.
        assert!(lib
            .create_group(&root, vec![ids[0].clone()], ids[0].clone(), vec![], vec![])
            .is_err());
        // Main must be a member.
        assert!(lib
            .create_group(&root, ids.clone(), "e".repeat(64), vec![], vec![])
            .is_err());
        let m = lib
            .create_group(
                &root,
                ids.clone(),
                ids[0].clone(),
                vec!["系列".into(), "系列".into()],
                vec!["常用".into()],
            )
            .unwrap();
        assert_eq!(m.groups.len(), 1);
        assert_eq!(m.groups[0].tags, vec!["系列"]);
        assert_eq!(
            manifest,
            fs::read(Path::new(&root).join("library.json")).unwrap()
        );
        // No re-grouping the same member.
        assert!(lib
            .create_group(&root, ids.clone(), ids[0].clone(), vec![], vec![])
            .is_err());
        drop(lib);
        let reopened = Library::open(Path::new(&root)).unwrap();
        let m = reopened.get_management().unwrap();
        assert_eq!(m.groups.len(), 1);
        let group_id = m.groups[0].id.clone();
        let m = reopened.disband_group(&root, &group_id).unwrap();
        assert!(m.groups.is_empty());
        assert!(reopened.disband_group(&root, &group_id).is_err());
        let _ = dir;
    }
    #[test]
    fn trashed_assets_cannot_join_groups_and_ignore_pairs_are_ordered() {
        let (dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        lib.set_trash(&root, vec![ids[0].clone()], true).unwrap();
        assert!(lib
            .create_group(&root, ids.clone(), ids[0].clone(), vec![], vec![])
            .is_err());
        lib.set_trash(&root, vec![ids[0].clone()], false).unwrap();
        let m = lib.ignore_pair(&root, &ids[1], &ids[0]).unwrap();
        let expected = if ids[0] < ids[1] {
            (ids[0].clone(), ids[1].clone())
        } else {
            (ids[1].clone(), ids[0].clone())
        };
        assert_eq!(m.ignored_pairs, vec![expected]);
        // Idempotent and canonical order.
        let m = lib.ignore_pair(&root, &ids[0], &ids[1]).unwrap();
        assert_eq!(m.ignored_pairs.len(), 1);
        assert!(lib.ignore_pair(&root, &ids[0], &ids[0]).is_err());
        let _ = dir;
    }
    #[test]
    fn group_metadata_edits_validate_and_persist() {
        let (dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        let m = lib
            .create_group(&root, ids.clone(), ids[0].clone(), vec![], vec![])
            .unwrap();
        let group_id = m.groups[0].id.clone();
        // Normalize, dedupe, persist.
        let m = lib
            .save_group_metadata(
                &root,
                &group_id,
                vec![" 系列 ".into(), "系列".into()],
                vec!["常用".into()],
            )
            .unwrap();
        assert_eq!(m.groups[0].tags, vec!["系列"]);
        assert_eq!(m.groups[0].collections, vec!["常用"]);
        drop(lib);
        let reopened = Library::open(Path::new(&root)).unwrap();
        let m = reopened.get_management().unwrap();
        assert_eq!(m.groups[0].tags, vec!["系列"]);
        let m = reopened
            .save_group_metadata(&root, &group_id, vec![], vec![])
            .unwrap();
        assert!(m.groups[0].tags.is_empty());
        assert!(reopened
            .save_group_metadata(&root, &"e".repeat(64), vec![], vec![])
            .is_err());
        assert!(reopened
            .save_group_metadata(&root, &group_id, vec!["bad\u{202E}tag".into()], vec![])
            .is_err());
        let _ = dir;
    }
    #[test]
    fn batch_rename_orders_numbers_and_skips_nothing_silently() {
        let (dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        let m = lib
            .batch_rename(&root, vec![ids[1].clone(), ids[0].clone()], "表情".into(), 5)
            .unwrap();
        assert_eq!(m.metadata[&ids[1]].name, "表情-005");
        assert_eq!(m.metadata[&ids[0]].name, "表情-006");
        // Re-running renames again in the same order (idempotent semantics).
        let m = lib
            .batch_rename(&root, vec![ids[0].clone()], "图".into(), 1)
            .unwrap();
        assert_eq!(m.metadata[&ids[0]].name, "图-001");
        drop(lib);
        let _ = dir;
    }
    #[test]
    fn batch_rename_validates_prefix_range_and_trash() {
        let (_dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        assert!(lib.batch_rename(&root, ids.clone(), "".into(), 1).is_err());
        assert!(lib.batch_rename(&root, ids.clone(), "  ".into(), 1).is_err());
        assert!(lib.batch_rename(&root, ids.clone(), "好".into(), 0).is_err());
        assert!(lib.batch_rename(&root, ids.clone(), "bad\u{202E}".into(), 1).is_err());
        lib.set_trash(&root, vec![ids[0].clone()], true).unwrap();
        assert!(lib.batch_rename(&root, ids.clone(), "好".into(), 1).is_err());
        assert!(lib.batch_rename(&root, vec![], "好".into(), 1).is_err());
    }
    #[test]
    fn batch_labels_adds_removes_and_keeps_member_names() {
        let (dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        lib.save_metadata(&root, ids[0].clone(), "已有名".into(), vec!["旧".into()], vec![])
            .unwrap();
        let m = lib
            .batch_labels(
                &root,
                ids.clone(),
                vec!["新".into(), "新".into()],
                vec!["旧".into()],
                vec!["合A".into()],
                vec![],
            )
            .unwrap();
        assert_eq!(m.metadata[&ids[0]].name, "已有名");
        assert_eq!(m.metadata[&ids[0]].tags, vec!["新"]);
        assert_eq!(m.metadata[&ids[0]].collections, vec!["合A"]);
        assert_eq!(m.metadata[&ids[1]].tags, vec!["新"]);
        // Removing everything from an otherwise empty record drops it.
        let m = lib
            .batch_labels(&root, vec![ids[1].clone()], vec![], vec!["新".into()], vec![], vec!["合A".into()])
            .unwrap();
        assert!(!m.metadata.contains_key(&ids[1]));
        drop(lib);
        let _ = dir;
    }
    #[test]
    fn batch_labels_rejects_empty_operations_and_bad_input() {
        let (_dir, lib, ids) = fixture();
        let root = lib.snapshot().root;
        assert!(lib
            .batch_labels(&root, ids.clone(), vec![], vec![], vec![], vec![])
            .is_err());
        assert!(lib
            .batch_labels(&root, vec![], vec!["a".into()], vec![], vec![], vec![])
            .is_err());
        assert!(lib
            .batch_labels(&root, ids.clone(), vec!["a\u{200B}".into()], vec![], vec![], vec![])
            .is_err());
        lib.set_trash(&root, vec![ids[0].clone()], true).unwrap();
        assert!(lib
            .batch_labels(&root, ids.clone(), vec!["a".into()], vec![], vec![], vec![])
            .is_err());
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
            .import_provenance(&lib.snapshot().root, &linked, "测试".into(), None, None)
            .is_err());
    }
}

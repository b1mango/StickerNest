use crate::{
    collection_runtime::CollectionRuntime,
    management::ManagementSnapshot,
    vision::DIMENSION,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use half::f16;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::Mutex,
};

pub const MODEL: &str = "laion-clip-vit-b32";
pub const MANIFEST_URL: &str =
    "https://github.com/b1mango/StickerNest/releases/download/classifier/manifest.json";
pub const BATCH_SIZE: usize = 16;
const CACHE_MAGIC: &[u8; 8] = b"SNEMBED\0";
const CACHE_VERSION: u32 = 1;
const CACHE_HEADER_BYTES: usize = 20;
const CACHE_ENTRY_BYTES: usize = 64 + DIMENSION * 4;
const MODEL_FILE: &str = "vision.safetensors";
const CHARACTERS_FILE: &str = "characters.json";

static OPERATION_LOCK: Mutex<()> = Mutex::new(());

pub fn operation_lock() -> &'static Mutex<()> {
    &OPERATION_LOCK
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Thresholds {
    #[serde(default = "default_high")]
    pub high: f32,
    #[serde(default = "default_low")]
    pub low: f32,
}

const fn default_high() -> f32 { 0.86 }
const fn default_low() -> f32 { 0.78 }

impl Default for Thresholds {
    fn default() -> Self {
        Self { high: default_high(), low: default_low() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Character {
    pub name: String,
    pub samples: Vec<Vec<f32>>,
    /// Overrides the pack-wide thresholds; characters differ in how tightly they cluster.
    pub thresholds: Option<Thresholds>,
}

pub const MIN_SAMPLES: usize = 2;
pub const MAX_SAMPLES: usize = 40;

fn thresholds_valid(t: &Thresholds) -> bool {
    t.high.is_finite() && t.low.is_finite()
        && (-1.0..=1.0).contains(&t.low) && (-1.0..=1.0).contains(&t.high) && t.low <= t.high
}

/// A character told apart from its image look-alike only by text in the picture,
/// e.g. 龙图 = a 熊猫头 whose caption contains 龙.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRule {
    pub name: String,
    pub from: String,
    pub contains: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CharacterPack {
    pub thresholds: Thresholds,
    pub characters: Vec<Character>,
    pub text_rules: Vec<TextRule>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawCharacterPack {
    version: u32,
    model: String,
    dim: usize,
    thresholds: Option<Thresholds>,
    characters: Vec<RawCharacter>,
    #[serde(default)]
    text_rules: Vec<TextRule>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawCharacter {
    name: String,
    samples: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thresholds: Option<Thresholds>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OutputCharacterPack {
    version: u32,
    model: &'static str,
    dim: usize,
    thresholds: Thresholds,
    characters: Vec<RawCharacter>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    text_rules: Vec<TextRule>,
}

impl CharacterPack {
    pub fn new(thresholds: Thresholds, characters: Vec<Character>) -> Result<Self, String> {
        if !thresholds_valid(&thresholds) {
            return Err("形象包阈值无效".into());
        }
        let mut names = HashSet::new();
        for character in &characters {
            let name_len = character.name.chars().count();
            if !(1..=50).contains(&name_len) || !names.insert(&character.name) {
                return Err("形象名称须为 1–50 字且不能重复".into());
            }
            if !(MIN_SAMPLES..=MAX_SAMPLES).contains(&character.samples.len()) {
                return Err(format!("形象“{}”需要 {MIN_SAMPLES}–{MAX_SAMPLES} 张样例", character.name));
            }
            if character.thresholds.as_ref().is_some_and(|t| !thresholds_valid(t)) {
                return Err(format!("形象“{}”阈值无效", character.name));
            }
            for sample in &character.samples {
                validate_vector(sample).map_err(|e| format!("形象“{}”样例无效：{e}", character.name))?;
            }
        }
        Ok(Self { thresholds, characters, text_rules: Vec::new() })
    }

    pub fn with_text_rules(mut self, rules: Vec<TextRule>) -> Result<Self, String> {
        let mut names: HashSet<&str> = self.characters.iter().map(|c| c.name.as_str()).collect();
        for rule in &rules {
            let len = rule.name.chars().count();
            if !(1..=50).contains(&len) || !names.insert(&rule.name) {
                return Err("文字规则名称须为 1–50 字且不能与形象重名".into());
            }
            if !self.characters.iter().any(|c| c.name == rule.from) {
                return Err(format!("文字规则“{}”的来源形象不存在", rule.name));
            }
            if rule.contains.is_empty() || rule.contains.iter().any(|t| t.trim().is_empty() || t.chars().count() > 20) {
                return Err(format!("文字规则“{}”的关键字无效", rule.name));
            }
        }
        self.text_rules = rules;
        Ok(self)
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        let raw: RawCharacterPack = serde_json::from_slice(bytes)
            .map_err(|e| format!("形象包不是有效 JSON：{e}"))?;
        if raw.version != 1 || raw.model != MODEL || raw.dim != DIMENSION {
            return Err("形象包版本、模型或向量维度不匹配".into());
        }
        let mut characters = Vec::with_capacity(raw.characters.len());
        for character in raw.characters {
            let mut samples = Vec::with_capacity(character.samples.len());
            for encoded in character.samples {
                let bytes = BASE64.decode(encoded).map_err(|e| format!("形象包样例编码无效：{e}"))?;
                if bytes.len() != DIMENSION * 2 {
                    return Err("形象包样例字节长度无效".into());
                }
                let vector = bytes.chunks_exact(2)
                    .map(|pair| f16::from_le_bytes([pair[0], pair[1]]).to_f32())
                    .collect();
                samples.push(vector);
            }
            characters.push(Character { name: character.name, samples, thresholds: character.thresholds });
        }
        Self::new(raw.thresholds.unwrap_or_default(), characters)?.with_text_rules(raw.text_rules)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, String> {
        let characters = self.characters.iter().map(|character| {
            let samples = character.samples.iter().map(|sample| {
                let mut bytes = Vec::with_capacity(DIMENSION * 2);
                for value in sample {
                    bytes.extend_from_slice(&f16::from_f32(*value).to_le_bytes());
                }
                BASE64.encode(bytes)
            }).collect();
            RawCharacter { name: character.name.clone(), samples, thresholds: character.thresholds }
        }).collect();
        serde_json::to_vec_pretty(&OutputCharacterPack {
            version: 1,
            model: MODEL,
            dim: DIMENSION,
            thresholds: self.thresholds,
            characters,
            text_rules: self.text_rules.clone(),
        }).map_err(|e| e.to_string())
    }

    pub fn names(&self) -> Vec<String> {
        self.characters.iter().map(|character| character.name.clone())
            .chain(self.text_rules.iter().map(|rule| rule.name.clone()))
            .collect()
    }

    fn rules_from<'a>(&'a self, base: &'a str) -> impl Iterator<Item = &'a TextRule> + 'a {
        self.text_rules.iter().filter(move |rule| rule.from == base)
    }
}

fn validate_vector(vector: &[f32]) -> Result<(), String> {
    if vector.len() != DIMENSION || vector.iter().any(|value| !value.is_finite()) {
        return Err("必须为 512 维有限数值".into());
    }
    if vector.iter().map(|value| value * value).sum::<f32>() <= 0.0 {
        return Err("向量范数必须大于零".into());
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifierStatus {
    pub enabled: bool,
    pub bytes: u64,
    pub characters: Vec<String>,
    pub model: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModelManifest {
    version: u32,
    model: String,
    files: Vec<ManifestFile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestFile {
    name: String,
    url: String,
    sha256: String,
    size: u64,
}

fn parse_manifest(bytes: &[u8]) -> Result<ModelManifest, String> {
    let manifest: ModelManifest = serde_json::from_slice(bytes)
        .map_err(|e| format!("模型清单不是有效 JSON：{e}"))?;
    if manifest.version != 1 || manifest.model != MODEL || manifest.files.len() != 2 {
        return Err("模型清单版本、型号或文件列表无效".into());
    }
    let mut names = HashSet::new();
    for file in &manifest.files {
        if !matches!(file.name.as_str(), MODEL_FILE | CHARACTERS_FILE) || !names.insert(&file.name) {
            return Err("模型清单文件名无效".into());
        }
        if !file.url.starts_with("https://github.com/")
            && !file.url.starts_with("https://objects.githubusercontent.com/")
            && !file.url.starts_with("https://release-assets.githubusercontent.com/")
        {
            return Err("模型清单包含未允许的下载地址".into());
        }
        if file.size == 0
            || file.sha256.len() != 64
            || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("模型清单文件大小或 SHA-256 无效".into());
        }
    }
    Ok(manifest)
}

fn regular_file_size(path: &Path) -> Result<Option<u64>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(Some(metadata.len())),
        Ok(_) => Err(format!("{} 必须是普通文件", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

pub fn status_at(directory: &Path) -> Result<ClassifierStatus, String> {
    let model_size = regular_file_size(&directory.join(MODEL_FILE))?;
    let characters_size = regular_file_size(&directory.join(CHARACTERS_FILE))?;
    let bytes = model_size.unwrap_or(0) + characters_size.unwrap_or(0);
    let characters = if characters_size.is_some() {
        CharacterPack::from_json(&fs::read(directory.join(CHARACTERS_FILE)).map_err(|e| e.to_string())?)?
            .names()
    } else {
        Vec::new()
    };
    let manifest_path = directory.join("manifest.json");
    if regular_file_size(&manifest_path)?.is_none() {
        return Ok(ClassifierStatus { enabled: false, bytes, characters, model: None });
    }
    let manifest = parse_manifest(&fs::read(manifest_path).map_err(|e| e.to_string())?)?;
    let enabled = manifest.files.iter().all(|file| {
        let actual = match file.name.as_str() {
            MODEL_FILE => model_size,
            CHARACTERS_FILE => characters_size,
            _ => unreachable!(),
        };
        actual == Some(file.size)
    });
    Ok(ClassifierStatus {
        enabled,
        bytes,
        characters,
        model: Some(manifest.model),
    })
}

fn embedding_cache_path(root: &Path, model: &str) -> PathBuf {
    root.join("cache").join(format!("embeddings-{model}.bin"))
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn read_embeddings(path: &Path) -> Result<BTreeMap<String, Vec<f32>>, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(format!("无法读取向量缓存：{error}")),
    };
    if bytes.len() < CACHE_HEADER_BYTES || &bytes[..8] != CACHE_MAGIC {
        return Err("向量缓存魔数无效".into());
    }
    let version = read_u32(&bytes, 8);
    let dimension = read_u32(&bytes, 12) as usize;
    let count = read_u32(&bytes, 16) as usize;
    if version != CACHE_VERSION || dimension != DIMENSION {
        return Err("向量缓存版本或维度无效".into());
    }
    let expected = CACHE_HEADER_BYTES.checked_add(count.checked_mul(CACHE_ENTRY_BYTES)
        .ok_or("向量缓存条目数溢出")?)
        .ok_or("向量缓存长度溢出")?;
    if bytes.len() != expected {
        return Err("向量缓存长度无效".into());
    }
    let mut embeddings = BTreeMap::new();
    for index in 0..count {
        let start = CACHE_HEADER_BYTES + index * CACHE_ENTRY_BYTES;
        let id_bytes = &bytes[start..start + 64];
        if !id_bytes.is_ascii() {
            return Err("向量缓存素材标识不是 ASCII".into());
        }
        let id = std::str::from_utf8(id_bytes).map_err(|e| e.to_string())?.to_string();
        let mut vector = Vec::with_capacity(DIMENSION);
        for offset in (start + 64..start + CACHE_ENTRY_BYTES).step_by(4) {
            vector.push(f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()));
        }
        validate_vector(&vector).map_err(|e| format!("向量缓存条目无效：{e}"))?;
        if embeddings.insert(id, vector).is_some() {
            return Err("向量缓存包含重复素材".into());
        }
    }
    Ok(embeddings)
}

fn write_embeddings(
    path: &Path,
    embeddings: &BTreeMap<String, Vec<f32>>,
) -> Result<(), String> {
    let parent = path.parent().ok_or("向量缓存路径无效")?;
    fs::create_dir_all(parent).map_err(|e| format!("无法创建向量缓存目录：{e}"))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temp.write_all(CACHE_MAGIC).map_err(|e| e.to_string())?;
    temp.write_all(&CACHE_VERSION.to_le_bytes()).map_err(|e| e.to_string())?;
    temp.write_all(&(DIMENSION as u32).to_le_bytes()).map_err(|e| e.to_string())?;
    temp.write_all(&(embeddings.len() as u32).to_le_bytes()).map_err(|e| e.to_string())?;
    for (id, vector) in embeddings {
        if id.len() != 64 || !id.is_ascii() {
            return Err("素材标识必须为 64 字节 ASCII".into());
        }
        temp.write_all(id.as_bytes()).map_err(|e| e.to_string())?;
        for value in vector {
            temp.write_all(&value.to_le_bytes()).map_err(|e| e.to_string())?;
        }
    }
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| format!("无法保存向量缓存：{}", e.error))?;
    Ok(())
}

pub fn update_embeddings(
    root: &Path,
    model: &str,
    items: &[(String, PathBuf)],
    mut embed: impl FnMut(&[PathBuf]) -> Result<Vec<Vec<f32>>, String>,
    mut on_progress: impl FnMut(usize, usize),
) -> Result<BTreeMap<String, Vec<f32>>, String> {
    let path = embedding_cache_path(root, model);
    let mut embeddings = read_embeddings(&path)?;
    let wanted: HashSet<&str> = items.iter().map(|(id, _)| id.as_str()).collect();
    embeddings.retain(|id, _| wanted.contains(id.as_str()));
    let missing: Vec<_> = items.iter().filter(|(id, _)| !embeddings.contains_key(id)).collect();
    let total = missing.len();
    let mut done = 0;
    for batch in missing.chunks(BATCH_SIZE) {
        let paths: Vec<_> = batch.iter().map(|(_, path)| path.clone()).collect();
        let vectors = embed(&paths)?;
        if vectors.len() != batch.len() {
            return Err("形象识别批量推理返回数量不匹配".into());
        }
        for ((id, _), vector) in batch.iter().zip(vectors) {
            validate_vector(&vector).map_err(|e| format!("推理向量无效：{e}"))?;
            embeddings.insert(id.clone(), vector);
            done += 1;
            if done % 8 == 0 || done == total {
                on_progress(done, total);
            }
        }
    }
    write_embeddings(&path, &embeddings)?;
    Ok(embeddings)
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot = a.iter().zip(b).map(|(a, b)| a * b).sum::<f32>();
    let a_norm = a.iter().map(|value| value * value).sum::<f32>().sqrt();
    let b_norm = b.iter().map(|value| value * value).sum::<f32>().sqrt();
    dot / (a_norm * b_norm)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DismissPair {
    pub asset_id: String,
    pub character: String,
}

fn read_dismissed(path: &Path) -> Result<BTreeSet<DismissPair>, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => return Err(format!("无法读取分类忽略列表：{error}")),
    };
    let pairs: Vec<DismissPair> = serde_json::from_slice(&bytes)
        .map_err(|e| format!("分类忽略列表无效：{e}"))?;
    if pairs.iter().any(|pair| pair.asset_id.len() != 64
        || !pair.asset_id.is_ascii()
        || !(1..=50).contains(&pair.character.chars().count()))
    {
        return Err("分类忽略列表包含无效条目".into());
    }
    Ok(pairs.into_iter().collect())
}

/// Recognized picture text per asset: `<root>/cache/ocr.json`. Images never change
/// under an id (content hash), so entries are computed once and kept.
fn texts_path(root: &Path) -> PathBuf {
    root.join("cache").join("ocr.json")
}

pub fn load_texts(root: &Path) -> BTreeMap<String, String> {
    fs::read(texts_path(root)).ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Recognizes text for `wanted` ids missing from the cache, in batches, and saves.
pub fn update_texts(
    root: &Path,
    wanted: &[(String, PathBuf)],
    mut recognize: impl FnMut(&[PathBuf]) -> Result<Vec<String>, String>,
) -> Result<BTreeMap<String, String>, String> {
    let mut texts = load_texts(root);
    let missing: Vec<&(String, PathBuf)> = wanted.iter().filter(|(id, _)| !texts.contains_key(id)).collect();
    if missing.is_empty() {
        return Ok(texts);
    }
    for batch in missing.chunks(24) {
        let paths: Vec<PathBuf> = batch.iter().map(|(_, path)| path.clone()).collect();
        let found = recognize(&paths)?;
        if found.len() != batch.len() {
            return Err("文字识别结果数量不符".into());
        }
        for ((id, _), text) in batch.iter().zip(found) {
            texts.insert(id.clone(), text);
        }
    }
    let path = texts_path(root);
    fs::create_dir_all(path.parent().ok_or("缓存路径无效")?).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut temp, &serde_json::to_vec(&texts).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    temp.persist(&path).map_err(|e| e.error.to_string())?;
    Ok(texts)
}

pub fn append_dismissed(root: &Path, pairs: Vec<DismissPair>) -> Result<(), String> {
    let cache = root.join("cache");
    fs::create_dir_all(&cache).map_err(|e| format!("无法创建分类缓存目录：{e}"))?;
    let path = cache.join("classify-dismissed.json");
    let mut dismissed = read_dismissed(&path)?;
    dismissed.extend(pairs);
    let pairs: Vec<_> = dismissed.into_iter().collect();
    let mut temp = tempfile::NamedTempFile::new_in(&cache).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut temp, &pairs).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| format!("无法保存分类忽略列表：{}", e.error))?;
    Ok(())
}

pub fn load_dismissed(root: &Path) -> Result<BTreeSet<DismissPair>, String> {
    read_dismissed(&root.join("cache").join("classify-dismissed.json"))
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub asset_id: String,
    pub character: String,
    pub score: f32,
    pub confident: bool,
    /// A look-alike collection (rule base or sibling rule) the asset is in by mistake,
    /// e.g. a plain 熊猫头 filed under 龙图; accepting moves it out of there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifyResponse {
    pub characters: Vec<String>,
    pub suggestions: Vec<Suggestion>,
}

fn character_score(
    asset_id: &str,
    vector: &[f32],
    character: &Character,
    collection_names: &[&str],
    embeddings: &BTreeMap<String, Vec<f32>>,
    management: &ManagementSnapshot,
) -> f32 {
    let mut best = character.samples.iter().map(|sample| cosine(vector, sample))
        .fold(f32::NEG_INFINITY, f32::max);
    for (member_id, member) in embeddings {
        if member_id != asset_id
            && management.metadata.get(member_id).is_some_and(|metadata| {
                metadata.collections.iter().any(|name| collection_names.contains(&name.as_str()))
            })
        {
            best = best.max(cosine(vector, member));
        }
    }
    best
}

/// Image score against a character, counting members of its own collection and of
/// collections of text rules derived from it (a 龙图 is still a 熊猫头 picture).
fn image_score(
    pack: &CharacterPack,
    asset_id: &str,
    vector: &[f32],
    character: &Character,
    embeddings: &BTreeMap<String, Vec<f32>>,
    management: &ManagementSnapshot,
) -> f32 {
    let names: Vec<&str> = std::iter::once(character.name.as_str())
        .chain(pack.rules_from(&character.name).map(|rule| rule.name.as_str()))
        .collect();
    character_score(asset_id, vector, character, &names, embeddings, management)
}

/// The name an asset gets for `character` once its recognized text is considered.
fn effective_name<'a>(pack: &'a CharacterPack, character: &'a Character, text: Option<&String>) -> &'a str {
    text.and_then(|text| pack.rules_from(&character.name)
            .find(|rule| rule.contains.iter().any(|needle| text.contains(needle.as_str()))))
        .map_or(character.name.as_str(), |rule| rule.name.as_str())
}

/// Assets whose best image match is a character with text rules; only these need OCR.
pub fn text_candidates(
    pack: &CharacterPack,
    embeddings: &BTreeMap<String, Vec<f32>>,
    management: &ManagementSnapshot,
) -> Vec<String> {
    if pack.text_rules.is_empty() {
        return Vec::new();
    }
    embeddings.iter().filter(|(asset_id, _)| !management.trash.contains_key(*asset_id)).filter_map(|(asset_id, vector)| {
        let (character, score) = pack.characters.iter()
            .map(|character| (character, image_score(pack, asset_id, vector, character, embeddings, management)))
            .max_by(|a, b| a.1.total_cmp(&b.1))?;
        let low = character.thresholds.unwrap_or(pack.thresholds).low;
        (score >= low && pack.rules_from(&character.name).next().is_some()).then(|| asset_id.clone())
    }).collect()
}

pub fn score_embeddings(
    pack: &CharacterPack,
    embeddings: &BTreeMap<String, Vec<f32>>,
    management: &ManagementSnapshot,
    dismissed: &BTreeSet<DismissPair>,
    texts: &BTreeMap<String, String>,
) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();
    for (asset_id, vector) in embeddings {
        if management.trash.contains_key(asset_id) {
            continue;
        }
        let mut best: Option<(&Character, &str, f32)> = None;
        for character in &pack.characters {
            let name = effective_name(pack, character, texts.get(asset_id));
            if management.metadata.get(asset_id).is_some_and(|metadata| {
                metadata.collections.iter().any(|collection| collection == name)
            }) {
                continue;
            }
            if dismissed.contains(&DismissPair {
                asset_id: asset_id.clone(),
                character: name.to_string(),
            }) {
                continue;
            }
            let score = image_score(pack, asset_id, vector, character, embeddings, management);
            let replace = match best {
                None => true,
                Some((_, other, best_score)) => score > best_score || (score == best_score && name < other),
            };
            if replace {
                best = Some((character, name, score));
            }
        }
        if let Some((character, name, score)) = best {
            let thresholds = character.thresholds.unwrap_or(pack.thresholds);
            if score >= thresholds.low {
                let replaces = std::iter::once(character.name.as_str())
                    .chain(pack.rules_from(&character.name).map(|rule| rule.name.as_str()))
                    .filter(|sibling| *sibling != name)
                    .find(|sibling| management.metadata.get(asset_id)
                        .is_some_and(|metadata| metadata.collections.iter().any(|c| c == sibling)))
                    .map(str::to_string);
                suggestions.push(Suggestion {
                    asset_id: asset_id.clone(),
                    character: name.to_string(),
                    score,
                    confident: score >= thresholds.high,
                    replaces,
                });
            }
        }
    }
    suggestions.sort_by(|a, b| {
        a.character.cmp(&b.character)
            .then_with(|| b.score.total_cmp(&a.score))
            .then_with(|| a.asset_id.cmp(&b.asset_id))
    });
    suggestions
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifierProgress {
    pub stage: String,
    pub message: String,
    pub done: u64,
    pub total: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireProgress {
    stage: String,
    message: String,
    done: u64,
    total: u64,
}

fn read_download_progress(
    child: &mut Child,
    on_progress: &mut impl FnMut(ClassifierProgress) -> Result<(), String>,
) -> Result<(VecDeque<String>, Option<String>), String> {
    let stderr = child.stderr.take().expect("模型下载进程的 stderr 已设置为管道");
    let mut errors = VecDeque::new();
    let mut progress_error = None;
    for line in BufReader::new(stderr).lines() {
        let line = line.map_err(|e| format!("读取模型下载进度失败：{e}"))?;
        if let Some(json) = line.strip_prefix("PROGRESS ") {
            match serde_json::from_str::<WireProgress>(json) {
                Ok(progress) => {
                    if let Err(error) = on_progress(ClassifierProgress {
                        stage: progress.stage,
                        message: progress.message,
                        done: progress.done,
                        total: progress.total,
                    }) {
                        progress_error.get_or_insert(error);
                    }
                }
                Err(error) => {
                    progress_error.get_or_insert_with(|| format!("模型下载进度格式无效：{error}"));
                }
            }
        } else {
            if errors.len() == 8 {
                errors.pop_front();
            }
            errors.push_back(line.chars().rev().take(600).collect::<String>().chars().rev().collect());
        }
    }
    Ok((errors, progress_error))
}

pub fn download_model(
    runtime: &CollectionRuntime,
    destination: &Path,
    mut on_progress: impl FnMut(ClassifierProgress) -> Result<(), String>,
) -> Result<(), String> {
    let mut child = runtime.python()
        .arg(runtime.script("download_model.py"))
        .arg(MANIFEST_URL)
        .arg(destination)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("无法启动内置模型下载运行时：{e}"))?;
    let progress = read_download_progress(&mut child, &mut on_progress);
    let status = child.wait().map_err(|e| format!("等待模型下载进程失败：{e}"))?;
    let (errors, progress_error) = progress?;
    if !status.success() {
        let detail = errors.into_iter().collect::<Vec<_>>().join("\n");
        return Err(if detail.trim().is_empty() {
            "形象识别模型下载失败".into()
        } else {
            format!("形象识别模型下载失败：{}", detail.chars().take(600).collect::<String>())
        });
    }
    if let Some(error) = progress_error {
        return Err(format!("发送模型下载进度失败：{error}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::management::AssetMetadata;

    fn vector(index: usize) -> Vec<f32> {
        let mut vector = vec![0.0; DIMENSION];
        vector[index] = 1.0;
        vector
    }

    fn numbered_vector(value: f32) -> Vec<f32> {
        let mut vector = vec![0.0; DIMENSION];
        vector[0] = value;
        vector[1] = 1.0;
        vector
    }

    fn pack_with(samples: Vec<Vec<f32>>) -> CharacterPack {
        CharacterPack {
            text_rules: Vec::new(),
            thresholds: Thresholds::default(),
            characters: vec![Character { name: "奶龙".into(), samples, thresholds: None }],
        }
    }

    fn id(value: u64) -> String {
        format!("{value:064x}")
    }

    #[test]
    fn text_rules_split_look_alikes_by_caption_and_share_samples() {
        let panda = Character { name: "熊猫头".into(), samples: vec![vector(0), vector(0)], thresholds: None };
        let rule = TextRule { name: "龙图".into(), from: "熊猫头".into(), contains: vec!["龙".into()] };
        let pack = CharacterPack::new(Thresholds { high: 0.9, low: 0.5 }, vec![panda.clone()]).unwrap()
            .with_text_rules(vec![rule.clone()]).unwrap();
        assert_eq!(pack.names(), vec!["熊猫头".to_string(), "龙图".to_string()]);
        let decoded = CharacterPack::from_json(&pack.to_json().unwrap()).unwrap();
        assert_eq!(decoded.text_rules, vec![rule.clone()]);
        // Rules must point at an existing character, not clash with names, and have keywords.
        let base = CharacterPack::new(Thresholds::default(), vec![panda.clone()]).unwrap();
        for bad in [
            TextRule { from: "不存在".into(), ..rule.clone() },
            TextRule { name: "熊猫头".into(), ..rule.clone() },
            TextRule { contains: vec![" ".into()], ..rule.clone() },
        ] {
            assert!(base.clone().with_text_rules(vec![bad]).is_err());
        }

        let (dragon, plain, member, other) = (id(1), id(2), id(3), id(4));
        let embeddings = BTreeMap::from([
            (dragon.clone(), vector(0)), (plain.clone(), vector(0)),
            (member.clone(), vector(0)), (other.clone(), vector(1)),
        ]);
        let mut management = ManagementSnapshot::default();
        // An existing 熊猫头 member whose caption says 龙 should move on to 龙图.
        management.metadata.insert(member.clone(), AssetMetadata { collections: vec!["熊猫头".into()], ..Default::default() });
        let mut candidates = text_candidates(&pack, &embeddings, &management);
        candidates.sort();
        assert_eq!(candidates, vec![dragon.clone(), plain.clone(), member.clone()]);
        let texts = BTreeMap::from([
            (dragon.clone(), "哦！懂了！ 聪明龙".to_string()),
            (plain.clone(), "老子早料到".to_string()),
            (member.clone(), "龙颜大悦".to_string()),
        ]);
        let by = |list: &[Suggestion], asset: &str| list.iter().find(|s| s.asset_id == asset).map(|s| s.character.clone());
        let suggestions = score_embeddings(&pack, &embeddings, &management, &BTreeSet::new(), &texts);
        assert_eq!(by(&suggestions, &dragon).as_deref(), Some("龙图"));
        assert_eq!(by(&suggestions, &plain).as_deref(), Some("熊猫头"));
        assert_eq!(by(&suggestions, &member).as_deref(), Some("龙图"));
        let moved = suggestions.iter().find(|s| s.asset_id == member).unwrap();
        assert_eq!(moved.replaces.as_deref(), Some("熊猫头"), "misfiled member moves out of the look-alike");
        assert!(suggestions.iter().filter(|s| s.asset_id != member).all(|s| s.replaces.is_none()));
        assert_eq!(by(&suggestions, &other), None);
        // Dismissing the derived name hides it; 龙图 members still count as panda samples.
        let dismissed = BTreeSet::from([DismissPair { asset_id: dragon.clone(), character: "龙图".into() }]);
        assert_eq!(by(&score_embeddings(&pack, &embeddings, &management, &dismissed, &texts), &dragon), None);
        let only_member = CharacterPack { characters: vec![Character { samples: vec![vector(1), vector(1)], ..panda }], ..pack.clone() };
        management.metadata.insert(dragon.clone(), AssetMetadata { collections: vec!["龙图".into()], ..Default::default() });
        let suggestions = score_embeddings(&only_member, &embeddings, &management, &BTreeSet::new(), &texts);
        assert_eq!(by(&suggestions, &plain).as_deref(), Some("熊猫头"), "龙图 members act as 熊猫头 samples");
    }

    #[test]
    fn recognized_text_is_cached_and_only_missing_ids_are_recognized() {
        let dir = tempfile::tempdir().unwrap();
        let wanted = vec![(id(1), PathBuf::from("/a.png")), (id(2), PathBuf::from("/b.png"))];
        let mut calls = Vec::new();
        let texts = update_texts(dir.path(), &wanted, |paths| { calls.push(paths.len()); Ok(paths.iter().map(|p| p.display().to_string()).collect()) }).unwrap();
        assert_eq!(texts[&id(2)], "/b.png");
        assert_eq!(load_texts(dir.path()), texts);
        let more = vec![(id(2), PathBuf::from("/b.png")), (id(3), PathBuf::from("/c.png"))];
        update_texts(dir.path(), &more, |paths| { calls.push(paths.len()); Ok(vec![String::new(); paths.len()]) }).unwrap();
        assert_eq!(calls, vec![2, 1]);
        assert!(update_texts(dir.path(), &[(id(4), PathBuf::from("/d.png"))], |_| Ok(vec![])).is_err());
    }

    #[test]
    fn character_pack_roundtrip_and_validation() {
        let original = CharacterPack::new(
            Thresholds { high: 0.9, low: 0.7 },
            vec![Character {
                name: "奶龙".into(),
                samples: vec![numbered_vector(0.25), numbered_vector(0.5), numbered_vector(1.0)],
                thresholds: None,
            }],
        ).unwrap();
        let encoded = original.to_json().unwrap();
        let decoded = CharacterPack::from_json(&encoded).unwrap();
        assert_eq!(decoded.names(), vec!["奶龙"]);
        assert_eq!(decoded.thresholds, original.thresholds);
        for (actual, expected) in decoded.characters[0].samples.iter().zip(&original.characters[0].samples) {
            for (actual, expected) in actual.iter().zip(expected) {
                assert!((actual - expected).abs() <= f16::from_f32(*expected).to_f32().abs().max(1.0) * 0.0006);
            }
        }
        let value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(value["model"], MODEL);
        assert_eq!(value["dim"], DIMENSION);

        for (field, replacement) in [
            ("model", serde_json::json!("other")),
            ("dim", serde_json::json!(511)),
            ("version", serde_json::json!(2)),
        ] {
            let mut invalid = value.clone();
            invalid[field] = replacement;
            assert!(CharacterPack::from_json(&serde_json::to_vec(&invalid).unwrap()).is_err());
        }
        for name in ["".to_string(), "长".repeat(51)] {
            assert!(CharacterPack::new(Thresholds::default(), vec![Character {
                name,
                samples: vec![vector(0), vector(1), vector(2)],
                thresholds: None,
            }]).is_err());
        }
        assert!(CharacterPack::new(Thresholds::default(), vec![Character {
            name: "样例不足".into(),
            samples: vec![vector(0)],
            thresholds: None,
        }]).is_err());
        let mut invalid = value.clone();
        invalid["characters"][0]["samples"][0] = serde_json::json!("not-base64");
        assert!(CharacterPack::from_json(&serde_json::to_vec(&invalid).unwrap()).is_err());
        let mut no_thresholds = value.clone();
        no_thresholds.as_object_mut().unwrap().remove("thresholds");
        assert_eq!(CharacterPack::from_json(&serde_json::to_vec(&no_thresholds).unwrap()).unwrap().thresholds, Thresholds::default());
    }

    #[test]
    fn embedding_cache_roundtrip_increment_and_cleanup() {
        let directory = tempfile::tempdir().unwrap();
        let items: Vec<_> = (0..17).map(|index| (id(index), PathBuf::from(format!("{index}.png")))).collect();
        let mut batches = Vec::new();
        let mut progress = Vec::new();
        let embeddings = update_embeddings(directory.path(), MODEL, &items, |paths| {
            batches.push(paths.len());
            Ok(paths.iter().map(|path| {
                let index = path.file_stem().unwrap().to_str().unwrap().parse::<u64>().unwrap();
                numbered_vector((index + 1) as f32)
            }).collect())
        }, |done, total| progress.push((done, total))).unwrap();
        assert_eq!(batches, vec![16, 1]);
        assert_eq!(progress, vec![(8, 17), (16, 17), (17, 17)]);
        assert_eq!(embeddings.len(), 17);
        let cache = embedding_cache_path(directory.path(), MODEL);
        assert_eq!(read_embeddings(&cache).unwrap(), embeddings);

        let changed: Vec<_> = items[1..].iter().cloned()
            .chain(std::iter::once((id(17), PathBuf::from("17.png"))))
            .collect();
        let mut computed = Vec::new();
        let updated = update_embeddings(directory.path(), MODEL, &changed, |paths| {
            computed.extend(paths.iter().cloned());
            Ok(paths.iter().map(|_| numbered_vector(18.0)).collect())
        }, |_, _| {}).unwrap();
        assert_eq!(computed, vec![PathBuf::from("17.png")]);
        assert_eq!(updated.len(), 17);
        assert!(!updated.contains_key(&id(0)));
        assert_eq!(updated[&id(17)][0], 18.0);
        assert_eq!(updated[&id(1)][0], 2.0);
        let no_work = update_embeddings(directory.path(), MODEL, &changed, |_| {
            panic!("完整缓存不应重新推理")
        }, |_, _| {}).unwrap();
        assert_eq!(no_work, updated);
    }

    #[test]
    fn scoring_uses_collection_members_and_excludes_self() {
        let pack = pack_with(vec![vector(0)]);
        let member = id(1);
        let query = id(2);
        let embeddings = BTreeMap::from([
            (member.clone(), vector(1)),
            (query.clone(), vector(1)),
        ]);
        let mut management = ManagementSnapshot::default();
        management.metadata.insert(member.clone(), AssetMetadata {
            collections: vec!["奶龙".into()], ..Default::default()
        });
        let suggestions = score_embeddings(&pack, &embeddings, &management, &BTreeSet::new(), &BTreeMap::new());
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].asset_id, query);
        assert_eq!(suggestions[0].character, "奶龙");
        assert!((suggestions[0].score - 1.0).abs() < 1e-6);
        assert!(suggestions[0].confident);

        let self_only = BTreeMap::from([(query.clone(), vector(1))]);
        let score = character_score(&query, &vector(1), &pack.characters[0], &["奶龙"], &self_only, &management);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn per_character_thresholds_override_pack_defaults() {
        let strict = Thresholds { high: 0.99, low: 0.98 };
        let pack = CharacterPack::new(Thresholds { high: 0.5, low: 0.1 }, vec![
            Character { name: "严格".into(), samples: vec![vector(0), vector(0)], thresholds: Some(strict) },
        ]).unwrap();
        let decoded = CharacterPack::from_json(&pack.to_json().unwrap()).unwrap();
        assert_eq!(decoded.characters[0].thresholds, Some(strict));
        assert!(CharacterPack::new(Thresholds::default(), vec![Character {
            name: "坏阈值".into(), samples: vec![vector(0), vector(1)],
            thresholds: Some(Thresholds { high: 0.5, low: 0.9 }),
        }]).is_err());
    }

    #[test]
    fn scoring_honors_membership_dismissal_thresholds_trash_and_order() {
        let pack = CharacterPack {
            text_rules: Vec::new(),
            thresholds: Thresholds { high: 0.86, low: 0.78 },
            characters: vec![
                Character { name: "甲".into(), samples: vec![vector(0)], thresholds: None },
                Character { name: "乙".into(), samples: vec![vector(1)], thresholds: None },
            ],
        };
        let already = id(1);
        let confident = id(2);
        let tentative = id(3);
        let ignored = id(4);
        let trashed = id(5);
        let other = id(6);
        let mut confident_vector = vector(0);
        confident_vector[1] = 0.25;
        let mut tentative_vector = vector(0);
        tentative_vector[1] = 0.75;
        let embeddings = BTreeMap::from([
            (already.clone(), vector(0)),
            (confident.clone(), confident_vector),
            (tentative.clone(), tentative_vector),
            (ignored.clone(), vector(0)),
            (trashed.clone(), vector(0)),
            (other.clone(), vector(1)),
        ]);
        let mut management = ManagementSnapshot::default();
        management.metadata.insert(already.clone(), AssetMetadata {
            collections: vec!["甲".into()], ..Default::default()
        });
        management.trash.insert(trashed.clone(), 1);
        let dismissed = BTreeSet::from([DismissPair { asset_id: ignored.clone(), character: "甲".into() }]);
        let suggestions = score_embeddings(&pack, &embeddings, &management, &dismissed, &BTreeMap::new());
        assert_eq!(suggestions.iter().map(|s| s.asset_id.as_str()).collect::<Vec<_>>(), vec![&other, &confident, &tentative]);
        assert!(suggestions[1].confident);
        assert!(!suggestions[2].confident);
        assert_eq!(suggestions[0].character, "乙");
        assert!(!suggestions.iter().any(|s| [&already, &ignored, &trashed].contains(&&s.asset_id)));
        assert_eq!(suggestions[1].character, suggestions[2].character);
        assert!(suggestions[1].score > suggestions[2].score);
    }

    fn manifest_json(model_size: u64, characters_size: u64) -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "model": MODEL,
            "files": [
                {"name": MODEL_FILE, "url": "https://github.com/b1mango/StickerNest/releases/download/classifier/vision.safetensors", "sha256": "a".repeat(64), "size": model_size},
                {"name": CHARACTERS_FILE, "url": "https://objects.githubusercontent.com/characters.json", "sha256": "b".repeat(64), "size": characters_size}
            ]
        })
    }

    #[test]
    fn manifest_validates_names_urls_and_status_sizes() {
        let valid = manifest_json(3, 4);
        assert!(parse_manifest(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for name in ["../model", "manifest.json", "other.bin"] {
            let mut invalid = valid.clone();
            invalid["files"][0]["name"] = serde_json::json!(name);
            assert!(parse_manifest(&serde_json::to_vec(&invalid).unwrap()).is_err());
        }
        for url in [
            "http://github.com/model",
            "https://evil.com/model",
            "https://github.com.evil.com/model",
            "https://objects.githubusercontent.com.evil.com/model",
        ] {
            let mut invalid = valid.clone();
            invalid["files"][0]["url"] = serde_json::json!(url);
            assert!(parse_manifest(&serde_json::to_vec(&invalid).unwrap()).is_err());
        }
        let mut duplicate = valid.clone();
        duplicate["files"][1]["name"] = serde_json::json!(MODEL_FILE);
        assert!(parse_manifest(&serde_json::to_vec(&duplicate).unwrap()).is_err());

        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join(MODEL_FILE), b"abc").unwrap();
        let pack = pack_with(vec![vector(0), vector(1), vector(2)]).to_json().unwrap();
        fs::write(directory.path().join(CHARACTERS_FILE), &pack).unwrap();
        let manifest = manifest_json(3, pack.len() as u64);
        fs::write(directory.path().join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
        let status = status_at(directory.path()).unwrap();
        assert!(status.enabled);
        assert_eq!(status.bytes, 3 + pack.len() as u64);
        assert_eq!(status.model.as_deref(), Some(MODEL));
        assert_eq!(status.characters, vec!["奶龙"]);
        fs::write(directory.path().join(MODEL_FILE), b"changed").unwrap();
        assert!(!status_at(directory.path()).unwrap().enabled);
    }

    #[test]
    fn dismissed_pairs_are_deduplicated_and_persisted() {
        let directory = tempfile::tempdir().unwrap();
        let first = DismissPair { asset_id: id(1), character: "奶龙".into() };
        let second = DismissPair { asset_id: id(2), character: "草地牛".into() };
        append_dismissed(directory.path(), vec![first.clone(), first.clone(), second.clone()]).unwrap();
        append_dismissed(directory.path(), vec![first.clone()]).unwrap();
        assert_eq!(load_dismissed(directory.path()).unwrap(), BTreeSet::from([first, second]));
        let stored: serde_json::Value = serde_json::from_slice(
            &fs::read(directory.path().join("cache/classify-dismissed.json")).unwrap(),
        ).unwrap();
        assert!(stored.is_array());
        assert_eq!(stored.as_array().unwrap().len(), 2);
        assert!(stored[0].get("assetId").is_some());
    }
}

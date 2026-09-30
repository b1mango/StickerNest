use fs2::FileExt;
use image::{AnimationDecoder, ImageDecoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 20 * 1024 * 1024;
const MAX_DIMENSION: u32 = 4096;
const MAX_ANIMATION_FRAMES: usize = 1000;
const MAX_ANIMATION_PIXELS: u64 = 100_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sticker {
    pub id: String,
    pub name: String,
    pub file_name: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub imported_at: u64,
    pub sources: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    pub root: String,
    pub items: Vec<Sticker>,
}

#[derive(Debug, Serialize)]
pub struct ImportFailure {
    pub name: String,
    pub error: String,
}

#[derive(Default, Debug, Serialize)]
pub struct ImportReport {
    pub added: usize,
    pub duplicates: usize,
    pub failed: Vec<ImportFailure>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    items: Vec<Sticker>,
}

pub struct Library {
    root: PathBuf,
    manifest: Manifest,
    // Holding this handle keeps the advisory exclusive lock for the library lifetime.
    _lock: File,
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn valid_source(source: &str) -> bool {
    matches!(source, "本地" | "微信" | "抖音")
}

fn regular_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(err)?;
    if !metadata.file_type().is_file() {
        return Err("必须是普通文件，不能是符号链接".into());
    }
    Ok(())
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    regular_file(path)?;
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(err)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("文件超过 20 MiB 限制".into());
    }
    Ok(bytes)
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_animation(frames: image::Frames<'_>) -> Result<(), String> {
    let mut pixels = 0_u64;
    for (index, frame) in frames.enumerate() {
        if index >= MAX_ANIMATION_FRAMES {
            return Err("动画超过本工具的 1000 帧限制".into());
        }
        let frame = frame.map_err(|e| format!("GIF 动画损坏：{e}"))?;
        pixels += u64::from(frame.buffer().width()) * u64::from(frame.buffer().height());
        if pixels > MAX_ANIMATION_PIXELS {
            return Err("动画超过本工具的累计 1 亿解码像素限制".into());
        }
    }
    Ok(())
}

fn inspect(bytes: &[u8]) -> Result<(String, u32, u32), String> {
    let format = image::guess_format(bytes).map_err(|_| "无法识别图片内容".to_string())?;
    let extension = match format {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Gif => "gif",
        image::ImageFormat::WebP => "webp",
        _ => return Err("仅支持 PNG、JPEG、GIF 和 WebP".into()),
    };
    match format {
        image::ImageFormat::Png => {
            let decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes)).map_err(err)?;
            if decoder.is_apng().map_err(err)? {
                return Err("第一版暂不支持 APNG 动画，请导入 GIF 动画或静态 PNG".into());
            }
        }
        image::ImageFormat::WebP => {
            let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).map_err(err)?;
            if decoder.has_animation() {
                return Err("第一版暂不支持动画 WebP，请导入 GIF 动画或静态 WebP".into());
            }
        }
        _ => (),
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|e| format!("图片损坏或尺寸超过 4096 × 4096：{e}"))?;
    if format == image::ImageFormat::Gif {
        let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).map_err(err)?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_DIMENSION);
        limits.max_image_height = Some(MAX_DIMENSION);
        limits.max_alloc = Some(128 * 1024 * 1024);
        decoder.set_limits(limits).map_err(err)?;
        // Stream validation, retaining only one decoded frame at a time.
        validate_animation(decoder.into_frames())?;
    }
    Ok((extension.into(), decoded.width(), decoded.height()))
}

impl Library {
    pub fn create(parent: &Path) -> Result<Self, String> {
        let parent = parent.canonicalize().map_err(err)?;
        if !parent.is_dir() {
            return Err("请选择文件夹".into());
        }
        let root = parent.join("StickerNest Library");
        fs::create_dir(&root)
            .map_err(|e| format!("无法创建资料库（请确认文件夹尚不存在）：{e}"))?;
        fs::create_dir(root.join("assets")).map_err(err)?;
        let lock = Self::acquire_lock(&root)?;
        let library = Self {
            root,
            manifest: Manifest {
                version: 1,
                items: vec![],
            },
            _lock: lock,
        };
        library.save(&library.manifest)?;
        Ok(library)
    }

    pub fn open(root: &Path) -> Result<Self, String> {
        if !fs::symlink_metadata(root)
            .map_err(err)?
            .file_type()
            .is_dir()
        {
            return Err("资料库必须是实际文件夹，不能是符号链接".into());
        }
        let root = root.canonicalize().map_err(err)?;
        Self::check_assets(&root)?;
        regular_file(&root.join("library.json"))?;
        let lock = Self::acquire_lock(&root)?;
        let manifest: Manifest = serde_json::from_slice(&read_bounded(&root.join("library.json"))?)
            .map_err(|e| format!("资料库索引损坏，未覆盖原文件：{e}"))?;
        Self::validate(&root, &manifest)?;
        Ok(Self {
            root,
            manifest,
            _lock: lock,
        })
    }

    fn check_assets(root: &Path) -> Result<(), String> {
        if !fs::symlink_metadata(root.join("assets"))
            .map_err(err)?
            .file_type()
            .is_dir()
        {
            return Err("assets 必须是资料库内的实际文件夹".into());
        }
        Ok(())
    }

    fn acquire_lock(root: &Path) -> Result<File, String> {
        let path = root.join(".library.lock");
        match fs::symlink_metadata(&path) {
            Ok(_) => regular_file(&path)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(err(e)),
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(err)?;
        file.try_lock_exclusive()
            .map_err(|_| "资料库正在被其他窗口或进程使用".to_string())?;
        Ok(file)
    }

    fn validate(root: &Path, manifest: &Manifest) -> Result<(), String> {
        if manifest.version != 1 {
            return Err("不支持此资料库版本".into());
        }
        let mut ids = HashSet::new();
        for item in &manifest.items {
            if item.id.len() != 64
                || !item
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || !matches!(item.format.as_str(), "png" | "jpg" | "gif" | "webp")
                || item.file_name != format!("{}.{}", item.id, item.format)
                || !ids.insert(&item.id)
                || item.width == 0
                || item.height == 0
                || item.width > MAX_DIMENSION
                || item.height > MAX_DIMENSION
                || item.bytes == 0
                || item.bytes > MAX_BYTES
                || item.sources.is_empty()
                || !item.sources.iter().all(|s| valid_source(s))
            {
                return Err("资料库索引含无效或不安全的素材记录".into());
            }
            regular_file(&root.join("assets").join(&item.file_name))?;
        }
        Ok(())
    }

    fn save(&self, manifest: &Manifest) -> Result<(), String> {
        let path = self.root.join("library.json");
        if fs::symlink_metadata(&path).is_ok() {
            regular_file(&path)?;
        }
        let mut temp = tempfile::NamedTempFile::new_in(&self.root).map_err(err)?;
        serde_json::to_writer_pretty(&mut temp, manifest).map_err(err)?;
        temp.write_all(b"\n").map_err(err)?;
        temp.as_file().sync_all().map_err(err)?;
        temp.persist(path).map_err(err)?;
        Ok(())
    }

    pub fn snapshot(&self) -> LibrarySnapshot {
        LibrarySnapshot {
            root: self.root.to_string_lossy().into_owned(),
            items: self.manifest.items.clone(),
        }
    }

    pub fn asset_path(&self, id: &str) -> Result<PathBuf, String> {
        Self::check_assets(&self.root)?;
        let item = self
            .manifest
            .items
            .iter()
            .find(|item| item.id == id)
            .ok_or_else(|| "素材不存在".to_string())?;
        let path = self.root.join("assets").join(&item.file_name);
        regular_file(&path)?;
        Ok(path)
    }

    pub fn import_files(
        &mut self,
        paths: Vec<PathBuf>,
        source: String,
    ) -> Result<ImportReport, String> {
        if !valid_source(&source) {
            return Err("来源必须为本地、微信或抖音".into());
        }
        Self::check_assets(&self.root)?;
        let mut report = ImportReport::default();
        for path in paths {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            match self.import_one(&path, &name, &source) {
                Ok(true) => report.added += 1,
                Ok(false) => report.duplicates += 1,
                Err(error) => report.failed.push(ImportFailure { name, error }),
            }
        }
        Ok(report)
    }

    fn import_one(&mut self, path: &Path, name: &str, source: &str) -> Result<bool, String> {
        Self::check_assets(&self.root)?;
        let bytes = read_bounded(path)?;
        let (format, width, height) = inspect(&bytes)?;
        let id = hash(&bytes);
        let mut next = Manifest {
            version: 1,
            items: self.manifest.items.clone(),
        };
        if let Some(item) = next.items.iter_mut().find(|item| item.id == id) {
            if hash(&read_bounded(
                &self.root.join("assets").join(&item.file_name),
            )?) != id
            {
                return Err("已有素材内容与索引不一致，请从备份恢复".into());
            }
            if !item.sources.iter().any(|s| s == source) {
                item.sources.push(source.into());
                self.save(&next)?;
                self.manifest = next;
            }
            return Ok(false);
        }
        let file_name = format!("{id}.{format}");
        let destination = self.root.join("assets").join(&file_name);
        match fs::symlink_metadata(&destination) {
            Ok(_) => {
                if hash(&read_bounded(&destination)?) != id {
                    return Err("目标文件已存在但内容不匹配，未覆盖".into());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut temp =
                    tempfile::NamedTempFile::new_in(self.root.join("assets")).map_err(err)?;
                temp.write_all(&bytes).map_err(err)?;
                temp.as_file().sync_all().map_err(err)?;
                temp.persist_noclobber(&destination).map_err(err)?;
            }
            Err(e) => return Err(err(e)),
        }
        next.items.push(Sticker {
            id,
            name: name.into(),
            file_name,
            format,
            width,
            height,
            bytes: bytes.len() as u64,
            imported_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(err)?
                .as_secs(),
            sources: vec![source.into()],
        });
        // A failed index write may leave an unreferenced asset; a retry safely reuses it.
        self.save(&next)?;
        self.manifest = next;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(path: &Path) -> Vec<u8> {
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 3)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let bytes = encoded.into_inner();
        fs::write(path, &bytes).unwrap();
        bytes
    }

    #[test]
    fn import_preserves_bytes_deduplicates_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("misleading.gif");
        let bytes = fixture(&source);
        let mut library = Library::create(dir.path()).unwrap();
        assert_eq!(
            library
                .import_files(vec![source.clone()], "微信".into())
                .unwrap()
                .added,
            1
        );
        assert_eq!(
            library
                .import_files(vec![source.clone()], "抖音".into())
                .unwrap()
                .duplicates,
            1
        );
        assert_eq!(
            library
                .import_files(vec![source.clone()], "抖音".into())
                .unwrap()
                .duplicates,
            1
        );
        let item = &library.snapshot().items[0];
        assert_eq!(item.sources, ["微信", "抖音"]);
        assert_eq!((item.width, item.height), (2, 3));
        assert_eq!(item.format, "png");
        assert_eq!(fs::read(&source).unwrap(), bytes);
        assert_eq!(
            fs::read(library.root.join("assets").join(&item.file_name)).unwrap(),
            bytes
        );
        let root = library.root.clone();
        drop(library);
        assert_eq!(Library::open(&root).unwrap().snapshot().items.len(), 1);
    }

    #[test]
    fn failed_files_do_not_block_valid_files() {
        let dir = tempfile::tempdir().unwrap();
        let invalid = dir.path().join("broken.png");
        fs::write(&invalid, b"not an image").unwrap();
        let valid = dir.path().join("ok.png");
        fixture(&valid);
        let mut library = Library::create(dir.path()).unwrap();
        let report = library
            .import_files(vec![invalid, valid], "本地".into())
            .unwrap();
        assert_eq!(report.added, 1);
        assert_eq!(report.failed.len(), 1);
        assert!(library.import_files(vec![], "unknown".into()).is_err());
    }

    #[test]
    fn creation_lock_and_corruption_are_non_destructive() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::create(dir.path()).unwrap();
        let root = library.root.clone();
        assert!(Library::create(dir.path()).is_err());
        assert!(Library::open(&root).is_err());
        drop(library);
        fs::write(root.join("library.json"), b"broken").unwrap();
        assert!(Library::open(&root).is_err());
        assert_eq!(fs::read(root.join("library.json")).unwrap(), b"broken");
    }

    #[test]
    fn unsafe_manifest_filename_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("ok.png");
        fixture(&source);
        let mut library = Library::create(dir.path()).unwrap();
        library.import_files(vec![source], "本地".into()).unwrap();
        library.manifest.items[0].file_name = "../../outside.png".into();
        library.save(&library.manifest).unwrap();
        let root = library.root.clone();
        drop(library);
        assert!(Library::open(&root).is_err());
    }

    #[test]
    fn animated_gif_is_kept_byte_for_byte() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("animated.gif");
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            encoder
                .encode_frame(image::Frame::new(image::RgbaImage::new(2, 3)))
                .unwrap();
            encoder
                .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                    2,
                    3,
                    image::Rgba([255, 0, 0, 255]),
                )))
                .unwrap();
        }
        fs::write(&source, &bytes).unwrap();
        let mut library = Library::create(dir.path()).unwrap();
        assert_eq!(
            library
                .import_files(vec![source], "本地".into())
                .unwrap()
                .added,
            1
        );
        let item = &library.snapshot().items[0];
        assert_eq!(item.format, "gif");
        assert_eq!(
            fs::read(library.asset_path(&item.id).unwrap()).unwrap(),
            bytes
        );
        assert!(library.asset_path("../../outside").is_err());
    }

    #[test]
    fn animation_frame_and_pixel_budgets_are_enforced() {
        let frames =
            (0..=MAX_ANIMATION_FRAMES).map(|_| Ok(image::Frame::new(image::RgbaImage::new(1, 1))));
        assert!(validate_animation(image::Frames::new(Box::new(frames)))
            .unwrap_err()
            .contains("1000 帧"));
        let frames = (0..7).map(|_| Ok(image::Frame::new(image::RgbaImage::new(4096, 4096))));
        assert!(validate_animation(image::Frames::new(Box::new(frames)))
            .unwrap_err()
            .contains("1 亿"));
    }

    #[test]
    fn apng_is_explicitly_rejected_instead_of_importing_only_its_default_image() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("animated.png");
        let mut bytes = fixture(&source);
        // A valid acTL chunk after IHDR marks a PNG with a separate default image.
        // We reject before attempting unsupported animation decoding.
        let animation_control = [
            0, 0, 0, 8, 97, 99, 84, 76, 0, 0, 0, 1, 0, 0, 0, 0, 180, 45, 233, 160,
        ];
        bytes.splice(33..33, animation_control);
        assert!(inspect(&bytes).unwrap_err().contains("暂不支持 APNG"));
    }

    #[test]
    fn oversized_file_and_dimensions_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let large = dir.path().join("large.png");
        File::create(&large)
            .unwrap()
            .set_len(MAX_BYTES + 1)
            .unwrap();
        let wide = dir.path().join("wide.png");
        image::DynamicImage::new_rgba8(MAX_DIMENSION + 1, 1)
            .save(&wide)
            .unwrap();
        let mut library = Library::create(dir.path()).unwrap();
        let report = library
            .import_files(vec![large, wide], "本地".into())
            .unwrap();
        assert_eq!(report.failed.len(), 2);
        assert_eq!(report.added, 0);
        assert!(library.snapshot().items.is_empty());
    }

    #[test]
    fn existing_asset_is_not_overwritten_on_hash_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("ok.png");
        let bytes = fixture(&source);
        let mut library = Library::create(dir.path()).unwrap();
        let target = library
            .root
            .join("assets")
            .join(format!("{}.png", hash(&bytes)));
        fs::write(&target, b"damaged").unwrap();
        assert_eq!(
            library
                .import_files(vec![source], "本地".into())
                .unwrap()
                .failed
                .len(),
            1
        );
        assert_eq!(fs::read(target).unwrap(), b"damaged");
        assert!(library.snapshot().items.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_asset_directory_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let library = Library::create(dir.path()).unwrap();
        let root = library.root.clone();
        drop(library);
        fs::remove_dir(root.join("assets")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("assets")).unwrap();
        assert!(Library::open(&root).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_source_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.png");
        fixture(&source);
        let link = dir.path().join("link.png");
        std::os::unix::fs::symlink(source, &link).unwrap();
        let mut library = Library::create(dir.path()).unwrap();
        assert_eq!(
            library
                .import_files(vec![link], "本地".into())
                .unwrap()
                .failed
                .len(),
            1
        );
    }
}

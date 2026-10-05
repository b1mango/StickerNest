//! Duplicate review: normalized-pixel identity plus perceptual fingerprints,
//! cached under cache/ and keyed by content hash and algorithm version. Static
//! images use an 8×8 identity grid plus aHash; animations are compared on
//! frame count, duration and three sampled frames — pairs are review
//! candidates only, never auto-merged, never compared against statics.
use crate::library::Library;
use image::{AnimationDecoder, ImageDecoder};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

const ALGORITHM_VERSION: u32 = 2;
const CACHE_FILE: &str = "similarity-cache.json";
const MAX_CACHE_ENTRIES: usize = 20_000;
const SIMILAR_DISTANCE: u32 = 6;
const MIN_DIMENSION: u32 = 8;
/// Sum of per-sample Hamming distances allowed for an animation candidate.
const ANIMATION_DISTANCE: u32 = 12;
const MAX_ANIMATION_FRAMES: usize = 1000;
const MAX_ANIMATION_PIXELS: u64 = 100_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fingerprints {
    pub version: u32,
    pub entries: HashMap<String, Fingerprint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fingerprint {
    /// SHA-256 of the normalized 8×8 RGBA-8888 pixel grid (first frame for animations).
    pub pixel_hash: String,
    /// 64-bit average hash over luma, computed with alpha flattened.
    pub a_hash: u64,
    pub has_alpha: bool,
    /// Present for animations; statics never carry this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<AnimationInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnimationInfo {
    pub frames: u32,
    pub duration_ms: u64,
    /// aHash of frames 0, middle and last (full canvas, disposal applied).
    pub samples: [u64; 3],
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all="camelCase")]
pub struct ScanReport {
    pub scanned_statics: usize,
    pub scanned_animations: usize,
    pub skipped_tiny: usize,
    pub failed: Vec<ScanFailure>,
    pub exact_groups: Vec<CandidateGroup>,
    pub similar_pairs: Vec<SimilarPair>,
    pub animation_pairs: Vec<AnimationPair>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all="camelCase")]
pub struct AnimationPair {
    pub base_id: String,
    pub other_id: String,
    pub distance: u32,
    pub frames: u32,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all="camelCase")]
pub struct ScanFailure {
    pub asset_id: String,
    pub error: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all="camelCase")]
pub struct CandidateGroup {
    pub pixel_hash: String,
    pub asset_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all="camelCase")]
pub struct SimilarPair {
    pub base_id: String,
    pub other_id: String,
    pub distance: u32,
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn hash_valid(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Average each 8×8 source region into an RGBA grid (identity digest) and a
/// luma grid (perceptual hash). A change confined to one region moves only
/// that grid cell; the rest of the picture stays bit-identical across rescale.
fn image_grids(image: &image::DynamicImage) -> Result<([u8; 256], [u8; 64], bool), String> {
    let (width, height) = (image.width(), image.height());
    if width < MIN_DIMENSION || height < MIN_DIMENSION {
        return Err("素材小于 8×8，不参与相似比较".into());
    }
    let rgba = image.to_rgba8();
    let mut sums = [[0_u32; 4]; 64];
    let mut luma = [0_u32; 64];
    let mut counts = [0_u32; 64];
    let mut alpha = false;
    let blend = |channel: u32, a: u32| (channel * a + 255 * (255 - a) + 127) / 255;
    for y in 0..height {
        let cell_y = (y * 8 / height) as usize;
        for x in 0..width {
            let cell = cell_y * 8 + (x * 8 / width) as usize;
            let [r, g, b, a] = rgba.get_pixel(x, y).0;
            if a < 255 {
                alpha = true;
            }
            // Fully transparent pixels keep alpha but zero RGB.
            let (r, g, b) = if a == 0 { (0, 0, 0) } else { (r, g, b) };
            sums[cell][0] += u32::from(r);
            sums[cell][1] += u32::from(g);
            sums[cell][2] += u32::from(b);
            sums[cell][3] += u32::from(a);
            let (r, g, b, a) = (u32::from(r), u32::from(g), u32::from(b), u32::from(a));
            luma[cell] += (299 * blend(r, a) + 587 * blend(g, a) + 114 * blend(b, a)) / 1000;
            counts[cell] += 1;
        }
    }
    let mut grid = [0_u8; 256];
    let mut luma_grid = [0_u8; 64];
    for cell in 0..64 {
        // counts[cell] ≥ 1 for every image ≥ 8×8 in both dimensions.
        let count = u64::from(counts[cell]);
        for channel in 0..4 {
            grid[cell * 4 + channel] = (sums[cell][channel] as u64 / count) as u8;
        }
        luma_grid[cell] = (luma[cell] as u64 / count) as u8;
    }
    Ok((grid, luma_grid, alpha))
}

fn average_hash(luma: &[u8; 64]) -> u64 {
    let average = luma.iter().map(|v| u32::from(*v)).sum::<u32>() / 64;
    let mut hash = 0_u64;
    for (index, value) in luma.iter().enumerate() {
        if u32::from(*value) > average {
            hash |= 1 << index;
        }
    }
    hash
}

fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

fn fingerprint_of(bytes: &[u8]) -> Result<Fingerprint, String> {
    let image = image::load_from_memory(bytes).map_err(|e| format!("无法解码素材：{e}"))?;
    let (grid, luma, has_alpha) = image_grids(&image)?;
    Ok(Fingerprint {
        pixel_hash: sha256(&grid),
        a_hash: average_hash(&luma),
        has_alpha,
        animation: None,
    })
}

/// Decode an animation to full-canvas frames (disposal/blend applied) and
/// fingerprint its frame count, total duration and three sample frames.
/// Import-time budgets are re-applied so pathological animations are bounded.
/// Returns (grid digest, luma aHash, has_alpha of the first frame, info).
fn animation_fingerprint_of(bytes: &[u8]) -> Result<(String, u64, bool, AnimationInfo), String> {
    let collect = |frames: image::Frames<'_>| -> Result<Vec<image::Frame>, String> {
        let mut out = Vec::new();
        let mut pixels = 0_u64;
        for (index, frame) in frames.into_iter().enumerate() {
            if index >= MAX_ANIMATION_FRAMES {
                return Err("动画超过本工具的 1000 帧限制".into());
            }
            let frame = frame.map_err(|e| format!("动画损坏：{e}"))?;
            pixels += u64::from(frame.buffer().width()) * u64::from(frame.buffer().height());
            if pixels > MAX_ANIMATION_PIXELS {
                return Err("动画超过本工具的累计 1 亿解码像素限制".into());
            }
            out.push(frame);
        }
        Ok(out)
    };
    let frames = match image::guess_format(bytes) {
        Ok(image::ImageFormat::Gif) => {
            let mut decoder =
                image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).map_err(err)?;
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(128 * 1024 * 1024);
            decoder.set_limits(limits).map_err(err)?;
            collect(decoder.into_frames())?
        }
        Ok(image::ImageFormat::WebP) => {
            let mut decoder =
                image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).map_err(err)?;
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(128 * 1024 * 1024);
            decoder.set_limits(limits).map_err(err)?;
            if !decoder.has_animation() {
                return Err("不是动画素材".into());
            }
            collect(decoder.into_frames())?
        }
        _ => return Err("不支持的动画格式".into()),
    };
    if frames.len() < 2 {
        return Err("单帧不是动画素材".into());
    }
    let mut duration_ms = 0_u64;
    for frame in &frames {
        let (numer, denom) = frame.delay().numer_denom_ms();
        // An unspecified delay behaves like the format default (~100 ms).
        duration_ms += if numer == 0 { 100 } else { (u64::from(numer) * 1000) / u64::from(denom.max(1)) };
    }
    let pick = [0, frames.len() / 2, frames.len() - 1];
    let mut samples = [0_u64; 3];
    let mut has_alpha = false;
    let mut first: Option<([u8; 256], [u8; 64])> = None;
    for (slot, index) in pick.iter().enumerate() {
        let image = image::DynamicImage::ImageRgba8(frames[*index].buffer().clone());
        let (grid, luma, alpha) = image_grids(&image)?;
        if alpha {
            has_alpha = true;
        }
        samples[slot] = average_hash(&luma);
        if slot == 0 {
            first = Some((grid, luma));
        }
    }
    let (grid, luma) = first.expect("first sample always exists");
    Ok((
        sha256(&grid),
        average_hash(&luma),
        has_alpha,
        AnimationInfo {
            frames: frames.len() as u32,
            duration_ms,
            samples,
        },
    ))
}

impl Library {
    fn cache_path(&self) -> PathBuf {
        PathBuf::from(self.snapshot().root)
            .join("cache")
            .join(CACHE_FILE)
    }

    fn read_fingerprints(&self) -> Result<Fingerprints, String> {
        let path = self.cache_path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Fingerprints {
                    version: ALGORITHM_VERSION,
                    entries: HashMap::new(),
                })
            }
            Err(e) => return Err(e.to_string()),
        };
        if !metadata.file_type().is_file() {
            return Err("相似指纹缓存异常（不是普通文件），已停止扫描".into());
        }
        let mut data = Vec::new();
        File::open(&path)
            .map_err(err)?
            .take(20 * 1024 * 1024)
            .read_to_end(&mut data)
            .map_err(err)?;
        let cache: Fingerprints =
            serde_json::from_slice(&data).map_err(|e| format!("指纹缓存损坏：{e}"))?;
        if cache.version != ALGORITHM_VERSION || cache.entries.len() > MAX_CACHE_ENTRIES {
            return Ok(Fingerprints {
                version: ALGORITHM_VERSION,
                entries: HashMap::new(),
            });
        }
        Ok(cache)
    }

    fn write_fingerprints(&self, cache: &Fingerprints) -> Result<(), String> {
        let cache_dir = PathBuf::from(self.snapshot().root).join("cache");
        match fs::symlink_metadata(&cache_dir) {
            Ok(metadata) if !metadata.file_type().is_dir() => {
                return Err("缓存目录异常，已停止扫描".into())
            }
            Ok(_) => (),
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            Err(_) => fs::create_dir(&cache_dir).map_err(err)?,
        }
        let path = self.cache_path();
        match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err("相似指纹缓存异常（不是普通文件），已停止扫描".into())
            }
            Ok(_) => (),
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            Err(_) => (),
        }
        let data = serde_json::to_vec(cache).map_err(err)?;
        let mut temp = tempfile::NamedTempFile::new_in(&cache_dir).map_err(err)?;
        temp.write_all(&data).map_err(err)?;
        temp.as_file().sync_all().map_err(err)?;
        temp.persist(path).map_err(err)?;
        Ok(())
    }

    /// Scan statics and animations. Exact groups are stable assets sharing
    /// normalized pixels; similar and animation pairs are review candidates,
    /// never merged automatically, reported without claiming transitivity.
    /// Ignored pairs and existing version groups are excluded; animations are
    /// only compared against other animations with matching frame counts.
    pub fn scan_static_duplicates(
        &self,
        ignored_pairs: &[(String, String)],
        grouped: &std::collections::HashSet<String>,
    ) -> Result<ScanReport, String> {
        let trashed: Vec<String> = {
            let management = self.get_management()?;
            management.trash.keys().cloned().collect()
        };
        let trashed: std::collections::HashSet<&str> =
            trashed.iter().map(String::as_str).collect();
        let ignored: std::collections::HashSet<(String, String)> = ignored_pairs
            .iter()
            .map(|(a, b)| {
                if a <= b {
                    (a.clone(), b.clone())
                } else {
                    (b.clone(), a.clone())
                }
            })
            .collect();
        let mut cache = self.read_fingerprints()?;
        let width_height: std::collections::HashMap<String, (u32, u32)> = self
            .snapshot()
            .items
            .iter()
            .map(|item| (item.id.clone(), (item.width, item.height)))
            .collect();
        let mut report = ScanReport {
            scanned_statics: 0,
            scanned_animations: 0,
            skipped_tiny: 0,
            failed: vec![],
            exact_groups: vec![],
            similar_pairs: vec![],
            animation_pairs: vec![],
        };
        let mut live: Vec<(String, Fingerprint)> = vec![];
        let mut animations: Vec<(String, Fingerprint)> = vec![];
        for item in self.snapshot().items {
            if trashed.contains(item.id.as_str()) {
                continue;
            }
            let bytes = match fs::read(self.asset_path(&item.id)?) {
                Ok(bytes) => bytes,
                Err(e) => {
                    report.failed.push(ScanFailure {
                        asset_id: item.id.clone(),
                        error: e.to_string(),
                    });
                    continue;
                }
            };
            let animated = is_animation(&bytes);
            let fingerprint = match cache.entries.get(&item.id) {
                Some(fingerprint)
                    if hash_valid(&fingerprint.pixel_hash)
                        && fingerprint.a_hash.count_ones() <= 64
                        && fingerprint.animation.is_some() == animated =>
                {
                    fingerprint.clone()
                }
                _ if animated => match animation_fingerprint_of(&bytes) {
                    Ok((digest, hash, has_alpha, info)) => {
                        let fingerprint = Fingerprint {
                            pixel_hash: digest,
                            a_hash: hash,
                            has_alpha,
                            animation: Some(info),
                        };
                        cache.entries.insert(item.id.clone(), fingerprint.clone());
                        fingerprint
                    }
                    Err(e) if e.contains("小于 8×8") => {
                        report.skipped_tiny += 1;
                        continue;
                    }
                    Err(e) => {
                        report.failed.push(ScanFailure {
                            asset_id: item.id.clone(),
                            error: e,
                        });
                        continue;
                    }
                },
                _ => match fingerprint_of(&bytes) {
                    Ok(fingerprint) => {
                        cache.entries.insert(item.id.clone(), fingerprint.clone());
                        fingerprint
                    }
                    Err(e) if e.contains("小于 8×8") => {
                        report.skipped_tiny += 1;
                        continue;
                    }
                    Err(e) => {
                        report.failed.push(ScanFailure {
                            asset_id: item.id.clone(),
                            error: e,
                        });
                        continue;
                    }
                },
            };
            if animated {
                report.scanned_animations += 1;
                animations.push((item.id.clone(), fingerprint));
            } else {
                report.scanned_statics += 1;
                live.push((item.id.clone(), fingerprint));
            }
        }
        cache.entries.retain(|id, _| {
            live.iter().any(|(live_id, _)| live_id == id)
                || animations.iter().any(|(anim_id, _)| anim_id == id)
        });
        self.write_fingerprints(&cache)?;

        // Exact identity also requires identical dimensions, illustrated by the
        // manifest: the 8×8 grid cannot tell a 16×16 from a 4096×4096 flat image.
        let mut by_pixel: HashMap<String, Vec<&str>> = HashMap::new();
        for (id, fingerprint) in &live {
            let (w, h) = width_height.get(id).copied().unwrap_or((0, 0));
            by_pixel
                .entry(format!("{w}x{h}:{}", fingerprint.pixel_hash))
                .or_default()
                .push(id.as_str());
        }
        let mut exact: Vec<CandidateGroup> = by_pixel
            .into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .map(|(pixel_hash, ids)| {
                let mut ids: Vec<String> = ids.into_iter().map(str::to_string).collect();
                ids.sort();
                CandidateGroup {
                    pixel_hash: pixel_hash.to_string(),
                    asset_ids: ids,
                }
            })
            .collect();
        exact.sort_by(|a, b| a.asset_ids.cmp(&b.asset_ids));
        report.exact_groups = exact;

        for (index, (base_id, base)) in live.iter().enumerate() {
            if grouped.contains(base_id) {
                continue;
            }
            for (other_id, other) in live.iter().skip(index + 1) {
                if grouped.contains(other_id) || base.pixel_hash == other.pixel_hash {
                    continue;
                }
                let key = if base_id <= other_id {
                    (base_id.clone(), other_id.clone())
                } else {
                    (other_id.clone(), base_id.clone())
                };
                if ignored.contains(&key) {
                    continue;
                }
                if base.has_alpha != other.has_alpha {
                    continue;
                }
                let distance = hamming(base.a_hash, other.a_hash);
                if distance > 0 && distance <= SIMILAR_DISTANCE {
                    report.similar_pairs.push(SimilarPair {
                        base_id: base_id.clone(),
                        other_id: other_id.clone(),
                        distance,
                    });
                }
            }
        }
        report.similar_pairs.sort_by(|a, b| {
            (a.distance, &a.base_id, &a.other_id).cmp(&(b.distance, &b.base_id, &b.other_id))
        });

        // Animation candidates: same frame count, duration within 10%, and the
        // sum of per-sample Hamming distances within budget — pairs only.
        for (index, (base_id, base)) in animations.iter().enumerate() {
            if grouped.contains(base_id) {
                continue;
            }
            let base_info = base.animation.as_ref().expect("animated fingerprint");
            for (other_id, other) in animations.iter().skip(index + 1) {
                if grouped.contains(other_id) {
                    continue;
                }
                let other_info = other.animation.as_ref().expect("animated fingerprint");
                if base_info.frames != other_info.frames
                    || base.has_alpha != other.has_alpha
                    || !duration_close(base_info.duration_ms, other_info.duration_ms)
                {
                    continue;
                }
                let key = if base_id <= other_id {
                    (base_id.clone(), other_id.clone())
                } else {
                    (other_id.clone(), base_id.clone())
                };
                if ignored.contains(&key) {
                    continue;
                }
                let distance: u32 = base_info
                    .samples
                    .iter()
                    .zip(other_info.samples.iter())
                    .map(|(a, b)| hamming(*a, *b))
                    .sum();
                if distance > 0 && distance <= ANIMATION_DISTANCE {
                    report.animation_pairs.push(AnimationPair {
                        base_id: base_id.clone(),
                        other_id: other_id.clone(),
                        distance,
                        frames: base_info.frames,
                        duration_ms: base_info.duration_ms.max(other_info.duration_ms),
                    });
                }
            }
        }
        report.animation_pairs.sort_by(|a, b| {
            (a.distance, &a.base_id, &a.other_id).cmp(&(b.distance, &b.base_id, &b.other_id))
        });
        Ok(report)
    }
}

/// Durations within 10% (relative to the larger one), never matching zero.
fn duration_close(a: u64, b: u64) -> bool {
    if a == 0 || b == 0 {
        return false;
    }
    let (larger, smaller) = if a >= b { (a, b) } else { (b, a) };
    (larger - smaller) * 10 <= larger
}

/// Detect animation by counting frames past the first, mirroring the formats
/// inspect() accepts. GIFs with a single frame are statics; the pixel budgets
/// enforced at import keep a full pass bounded.
fn is_animation(bytes: &[u8]) -> bool {
    match image::guess_format(bytes) {
        Ok(image::ImageFormat::Gif) => {
            image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
                .map(|decoder| decoder.into_frames().nth(1).is_some())
                .unwrap_or(false)
        }
        Ok(image::ImageFormat::WebP) => {
            image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))
                .map(|decoder| decoder.has_animation() && decoder.into_frames().nth(1).is_some())
                .unwrap_or(false)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    fn fixture() -> (tempfile::TempDir, Library) {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::create(dir.path()).unwrap();
        (dir, library)
    }
    fn save_png(path: &Path, pixels: image::RgbaImage) {
        pixels.save(path).unwrap();
    }
    fn import(library: &mut Library, files: Vec<PathBuf>) -> Vec<String> {
        library.import_files(files, "本地".into()).unwrap();
        library
            .snapshot()
            .items
            .iter()
            .map(|i| i.id.clone())
            .collect()
    }
    fn red(width: u32, height: u32) -> image::RgbaImage {
        image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]))
    }
    fn crc32(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }
    #[test]
    fn same_pixels_different_encoding_group_together() {
        let (dir, mut library) = fixture();
        let png = dir.path().join("a.png");
        save_png(&png, red(16, 16));
        let jpg_pixels = image::DynamicImage::ImageRgba8(red(16, 16));
        let mut jpg_bytes = Cursor::new(Vec::new());
        jpg_pixels
            .write_to(&mut jpg_bytes, image::ImageFormat::Jpeg)
            .unwrap();
        let jpg = dir.path().join("b.jpg");
        fs::write(&jpg, jpg_bytes.into_inner()).unwrap();
        let ids = import(&mut library, vec![png, jpg]);
        assert_eq!(ids.len(), 2);
        let report = library
            .scan_static_duplicates(&[], &HashSet::new())
            .unwrap();
        // JPEG compression changes pixels; identical pixels only group exactly.
        assert!(report.exact_groups.is_empty());
        assert_eq!(report.scanned_statics, 2);
    }
    #[test]
    fn byte_identical_reencode_keeps_exact_group_and_alpha_separates() {
        let (dir, mut library) = fixture();
        let png_a = dir.path().join("a.png");
        save_png(&png_a, red(16, 16));
        // Identical pixels under a different encoding: separate asset (exact
        // byte dedup cannot merge these), grouped by the pixel grid digest.
        let gif_b = dir.path().join("b.gif");
        let mut gif_bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut gif_bytes);
            encoder.encode_frame(image::Frame::new(red(16, 16))).unwrap();
        }
        fs::write(&gif_b, &gif_bytes).unwrap();
        // An opaque checkerboard: different pixels, third asset.
        let mut checker = image::RgbaImage::new(16, 16);
        for y in 0..16 {
            for x in 0..16 {
                let color = if (x + y) % 2 == 0 {
                    [255, 0, 0, 255]
                } else {
                    [254, 1, 0, 255]
                };
                checker.put_pixel(x, y, image::Rgba(color));
            }
        }
        let checker_path = dir.path().join("checker.png");
        save_png(&checker_path, checker);
        let ids = import(&mut library, vec![png_a, gif_b, checker_path]);
        assert_eq!(ids.len(), 3);
        let report = library
            .scan_static_duplicates(&[], &HashSet::new())
            .unwrap();
        let group = report
            .exact_groups
            .iter()
            .find(|g| g.asset_ids.len() == 2)
            .expect("identical pixels across encodings should group");
        assert!(group.asset_ids.contains(&ids[0]) && group.asset_ids.contains(&ids[1]));
        assert!(!group.asset_ids.contains(&ids[2]));
        // The checkerboard is opaque like flat red, so pixels differ.
        assert!(report
            .similar_pairs
            .iter()
            .all(|p| p.base_id != ids[2] && p.other_id != ids[2]));
        // Fully transparent red and black pixels normalize identically and
        // therefore share one asset (documented behavior, not 3 assets).
        let transparent_red = dir.path().join("tr.png");
        save_png(
            &transparent_red,
            image::RgbaImage::from_pixel(16, 16, image::Rgba([255, 0, 0, 0])),
        );
        let transparent_black = dir.path().join("tb.png");
        save_png(
            &transparent_black,
            image::RgbaImage::from_pixel(16, 16, image::Rgba([0, 0, 0, 0])),
        );
        let ids = import(&mut library, vec![transparent_red, transparent_black]);
        // Byte-exact dedup keeps them separate (3+2), since assets are hash-named.
        assert_eq!(ids.len(), 5);
    }
    #[test]
    fn animations_and_tiny_images_are_skipped_and_cached() {
        let (dir, mut library) = fixture();
        // Animated GIF with two frames.
        let gif = dir.path().join("anim.gif");
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
                encoder
                    .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                        16,
                        16,
                        image::Rgba(color),
                    )))
                    .unwrap();
            }
        }
        fs::write(&gif, bytes).unwrap();
        let tiny = dir.path().join("tiny.png");
        save_png(&tiny, red(4, 4));
        let normal = dir.path().join("normal.png");
        save_png(&normal, red(16, 16));
        import(&mut library, vec![gif, tiny, normal]);
        let report = library
            .scan_static_duplicates(&[], &HashSet::new())
            .unwrap();
        assert_eq!(report.scanned_animations, 1);
        assert_eq!(report.skipped_tiny, 1);
        assert_eq!(report.scanned_statics, 1);
        let cache_path = library.cache_path();
        assert!(cache_path.exists());
        let before: Fingerprints = serde_json::from_slice(&fs::read(&cache_path).unwrap()).unwrap();
        library
            .scan_static_duplicates(&[], &HashSet::new())
            .unwrap();
        let after: Fingerprints = serde_json::from_slice(&fs::read(&cache_path).unwrap()).unwrap();
        // Re-scanning reuses the cache: identical entries, no rewriting.
        assert_eq!(before.version, after.version);
        assert_eq!(before.entries.len(), after.entries.len());
        for (id, fingerprint) in &before.entries {
            let next = &after.entries[id];
            assert_eq!(fingerprint.pixel_hash, next.pixel_hash);
            assert_eq!(fingerprint.a_hash, next.a_hash);
            assert_eq!(fingerprint.has_alpha, next.has_alpha);
            assert_eq!(
                fingerprint.animation.as_ref().map(|a| (a.frames, a.duration_ms, a.samples)),
                next.animation.as_ref().map(|a| (a.frames, a.duration_ms, a.samples))
            );
        }
    }
    fn red_one_pixel_delta() -> image::RgbaImage {
        // Checkerboard base (evades encoders' flat-color optimizations), with
        // its top-left quadrant recolored mid-dark: pairs of this pattern with
        // a tiny local brightening land within the similarity threshold.
        let mut pixels = image::RgbaImage::new(16, 16);
        for y in 0..16 {
            for x in 0..16 {
                let color = if (x + y) % 2 == 0 {
                    [255, 0, 0, 255]
                } else {
                    [254, 1, 0, 255]
                };
                pixels.put_pixel(x, y, image::Rgba(color));
            }
        }
        for y in 0..8 {
            for x in 0..8 {
                let color = if (x + y) % 2 == 0 {
                    [129, 0, 0, 255]
                } else {
                    [128, 1, 0, 255]
                };
                pixels.put_pixel(x, y, image::Rgba(color));
            }
        }
        pixels
    }
    #[test]
    fn ignored_pairs_are_excluded_and_grouped_members_not_bases() {
        let (dir, mut library) = fixture();
        // Two opaque images that differ only in one corner region: close but
        // not pixel-identical, so they become similarity review candidates.
        let a = dir.path().join("a.png");
        save_png(&a, red_one_pixel_delta());
        let mut b_pixels = red_one_pixel_delta();
        // Small local brightening of a 2×2 block in the unchanged region.
        b_pixels.put_pixel(14, 14, image::Rgba([200, 0, 0, 255]));
        b_pixels.put_pixel(15, 14, image::Rgba([200, 0, 0, 255]));
        b_pixels.put_pixel(14, 15, image::Rgba([200, 0, 0, 255]));
        b_pixels.put_pixel(15, 15, image::Rgba([200, 0, 0, 255]));
        let b = dir.path().join("b.png");
        save_png(&b, b_pixels);
        let ids = import(&mut library, vec![a, b]);
        let report = library
            .scan_static_duplicates(&[], &HashSet::new())
            .unwrap();
        assert_eq!(report.similar_pairs.len(), 1, "expected one similar pair: {report:?}");
        let pair = &report.similar_pairs[0];
        assert!(pair.distance > 0 && pair.distance <= 6, "distance={}", pair.distance);
        let report_ignored = library
            .scan_static_duplicates(&[(pair.base_id.clone(), pair.other_id.clone())], &HashSet::new())
            .unwrap();
        assert!(report_ignored.similar_pairs.is_empty());
        let grouped: HashSet<String> = [pair.base_id.clone()].into_iter().collect();
        let report_grouped = library
            .scan_static_duplicates(&[], &grouped)
            .unwrap();
        assert!(report_grouped.similar_pairs.is_empty());
        assert!(ids.iter().all(|id| !id.is_empty()));
    }
    #[test]
    fn non_multiple_dimensions_average_their_real_pixel_counts() {
        // 17×9: irregular cells. The cell averages must use the actual count
        // of pixels assigned to each cell, not a floor-divided boundary count.
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            17,
            9,
            image::Rgba([255, 255, 255, 255]),
        ));
        let (grid, luma, alpha) = image_grids(&image).unwrap();
        assert!(!alpha);
        assert!(grid.chunks(4).all(|cell| cell == [255, 255, 255, 255]));
        assert!(luma.iter().all(|value| *value == 255));
        // Same dimensions and pixels but as JPEG (lossy) does not group with
        // the original; a same-pixel PNG at a different size must not either.
        let bigger = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            34,
            18,
            image::Rgba([255, 255, 255, 255]),
        ));
        let (big_grid, _, _) = image_grids(&bigger).unwrap();
        assert_eq!(grid, big_grid, "flat grids match across sizes");
        // Dimensions therefore belong to the exact-identity key, not the grid.
    }
    #[test]
    fn exact_identity_requires_same_dimensions() {
        let (dir, mut library) = fixture();
        let small = dir.path().join("small.png");
        save_png(&small, red(16, 16));
        let large = dir.path().join("large.png");
        save_png(&large, red(32, 32));
        let ids = import(&mut library, vec![small, large]);
        assert_eq!(ids.len(), 2);
        let report = library
            .scan_static_duplicates(&[], &HashSet::new())
            .unwrap();
        assert!(report.exact_groups.is_empty(), "different sizes must not group");
    }
    #[test]
    fn grouped_assets_cannot_be_trashed_before_disband() {
        let (dir, mut library) = fixture();
        let a = dir.path().join("a.png");
        save_png(&a, red(16, 16));
        let mut b = red(16, 16);
        b.put_pixel(15, 15, image::Rgba([254, 1, 0, 255]));
        let b_path = dir.path().join("b.png");
        save_png(&b_path, b);
        let ids = import(&mut library, vec![a, b_path]);
        let root = library.snapshot().root;
        library
            .create_group(&root, ids.clone(), ids[0].clone(), vec![], vec![])
            .unwrap();
        assert!(library
            .set_trash(&root, vec![ids[0].clone()], true)
            .is_err());
        assert!(library
            .set_trash(&root, vec![ids[1].clone()], true)
            .is_err());
        let group_id = library.get_management().unwrap().groups[0].id.clone();
        library.disband_group(&root, &group_id).unwrap();
        library
            .set_trash(&root, vec![ids[0].clone()], true)
            .unwrap();
    }
    fn two_frame_gif(path: &Path, first: [u8; 4], second: [u8; 4]) {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            for color in [first, second] {
                encoder
                    .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                        16,
                        16,
                        image::Rgba(color),
                    )))
                    .unwrap();
            }
        }
        fs::write(path, bytes).unwrap();
    }
    fn four_frame_gif(path: &Path) {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            for color in [
                [255, 0, 0, 255],
                [0, 255, 0, 255],
                [255, 0, 0, 255],
                [0, 255, 0, 255],
            ] {
                encoder
                    .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                        16,
                        16,
                        image::Rgba(color),
                    )))
                    .unwrap();
            }
        }
        fs::write(path, bytes).unwrap();
    }
    #[test]
    fn near_identical_animations_form_a_candidate_pair() {
        let (dir, mut library) = fixture();
        // Two-frame animation [red, black].
        let a = dir.path().join("a2.gif");
        two_frame_gif(&a, [255, 0, 0, 255], [0, 0, 0, 255]);
        // Same two frames, second frame brightened in one corner: samples
        // mostly match — a review candidate, and animations never enter the
        // static exact/similar results.
        let b = dir.path().join("b2.gif");
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            encoder
                .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                    16,
                    16,
                    image::Rgba([255, 0, 0, 255]),
                )))
                .unwrap();
            let mut dark = image::RgbaImage::from_pixel(16, 16, image::Rgba([0, 0, 0, 255]));
            dark.put_pixel(15, 15, image::Rgba([40, 40, 40, 255]));
            encoder.encode_frame(image::Frame::new(dark)).unwrap();
        }
        fs::write(&b, bytes).unwrap();
        import(&mut library, vec![a, b]);
        let report = library
            .scan_static_duplicates(&[], &HashSet::new())
            .unwrap();
        assert_eq!(report.scanned_animations, 2);
        assert_eq!(report.animation_pairs.len(), 1, "{:?}", report.animation_pairs);
        assert!(report.animation_pairs[0].distance > 0 && report.animation_pairs[0].distance <= 12);
        assert_eq!(report.animation_pairs[0].frames, 2);
        assert!(report.exact_groups.is_empty());
        assert!(report.similar_pairs.is_empty());
    }
    #[test]
    fn different_frame_counts_do_not_pair() {
        let (dir, mut library) = fixture();
        let two = dir.path().join("two.gif");
        two_frame_gif(&two, [255, 0, 0, 255], [0, 255, 0, 255]);
        let four = dir.path().join("four.gif");
        four_frame_gif(&four);
        import(&mut library, vec![two, four]);
        let report = library
            .scan_static_duplicates(&[], &HashSet::new())
            .unwrap();
        assert_eq!(report.scanned_animations, 2);
        assert!(report.animation_pairs.is_empty());
    }
    #[test]
    fn animations_and_statics_never_pair() {
        let (dir, mut library) = fixture();
        let gif = dir.path().join("anim.gif");
        two_frame_gif(&gif, [255, 0, 0, 255], [255, 0, 0, 255]);
        let png = dir.path().join("flat.png");
        save_png(&png, red(16, 16));
        import(&mut library, vec![gif, png]);
        let report = library
            .scan_static_duplicates(&[], &HashSet::new())
            .unwrap();
        assert_eq!(report.scanned_animations, 1);
        assert_eq!(report.scanned_statics, 1);
        assert!(report.animation_pairs.is_empty());
        assert!(report.similar_pairs.is_empty());
        assert!(report.exact_groups.is_empty());
    }
    #[test]
    fn trashed_assets_are_not_scanned() {
        let (dir, mut library) = fixture();
        let a = dir.path().join("a.png");
        save_png(&a, red(16, 16));
        let ids = import(&mut library, vec![a]);
        library
            .set_trash(&library.snapshot().root, vec![ids[0].clone()], true)
            .unwrap();
        let report = library
            .scan_static_duplicates(&[], &HashSet::new())
            .unwrap();
        assert_eq!(report.scanned_statics, 0);
    }
}

use crate::library::Library;
use image::{AnimationDecoder, ImageEncoder};
use serde::Serialize;
use std::{
    fs,
    io::BufReader,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedWechat {
    pub paths: Vec<String>,
    pub converted: usize,
    pub oversized: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardResult {
    pub count: usize,
    pub converted: usize,
    pub oversized: Vec<String>,
}

const DRAG_ICON_SIZE: u32 = 72;

pub(crate) fn real_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_dir() => Ok(()),
        Ok(_) => Err(format!("必须是实际文件夹：{}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|e| e.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

impl Library {
    pub(crate) fn validate_selection(&self, ids: &[String]) -> Result<Vec<PathBuf>, String> {
        if ids.is_empty() || ids.len() > 20_000 {
            return Err("请选择 1–20000 个素材".into());
        }
        ids.iter().map(|id| self.asset_path(id)).collect()
    }

    pub fn prepare_wechat(
        &self,
        expected_root: &str,
        asset_ids: &[String],
    ) -> Result<PreparedWechat, String> {
        self.check_expected_root(expected_root)?;
        let originals = self.validate_selection(asset_ids)?;
        let mut result = PreparedWechat {
            paths: vec![],
            converted: 0,
            oversized: vec![],
        };
        for (id, original) in asset_ids.iter().zip(originals) {
            let path = if original.extension().and_then(|s| s.to_str()) == Some("webp") {
                let cache = Path::new(expected_root).join("cache");
                real_directory(&cache)?;
                let cache = cache.join("wechat");
                real_directory(&cache)?;
                let mut cached = None;
                for ext in ["png", "gif"] {
                    let path = cache.join(format!("{id}.{ext}"));
                    match fs::symlink_metadata(&path) {
                        Ok(meta) if meta.file_type().is_file() => {
                            cached = Some(path);
                            break;
                        }
                        Ok(_) => return Err("微信缓存必须是普通文件".into()),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                        Err(e) => return Err(e.to_string()),
                    }
                }
                let path = if let Some(path) = cached {
                    path
                } else {
                    let decoder = image::codecs::webp::WebPDecoder::new(BufReader::new(
                        fs::File::open(&original).map_err(|e| e.to_string())?,
                    ))
                    .map_err(|e| format!("WebP 解码失败：{e}"))?;
                    let animated = decoder.has_animation();
                    let path = cache.join(format!("{id}.{}", if animated { "gif" } else { "png" }));
                    let mut temp =
                        tempfile::NamedTempFile::new_in(&cache).map_err(|e| e.to_string())?;
                    if animated {
                        // Speed 10 is the gif crate's recommended trade-off; speed 1 makes
                        // long animations take seconds to quantize on first copy.
                        let mut encoder = image::codecs::gif::GifEncoder::new_with_speed(&mut temp, 10);
                        encoder
                            .set_repeat(image::codecs::gif::Repeat::Infinite)
                            .map_err(|e| e.to_string())?;
                        encoder
                            .try_encode_frames(decoder.into_frames())
                            .map_err(|e| format!("WebP 动画转换失败：{e}"))?;
                    } else {
                        let image = image::DynamicImage::from_decoder(decoder)
                            .map_err(|e| e.to_string())?
                            .to_rgba8();
                        image::codecs::png::PngEncoder::new(&mut temp)
                            .write_image(
                                &image,
                                image.width(),
                                image.height(),
                                image::ExtendedColorType::Rgba8,
                            )
                            .map_err(|e| e.to_string())?;
                    }
                    temp.as_file().sync_all().map_err(|e| e.to_string())?;
                    temp.persist(&path).map_err(|e| e.to_string())?;
                    path
                };
                result.converted += 1;
                path
            } else {
                original
            };
            if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 1024 * 1024 {
                result.oversized.push(id.clone());
            }
            result.paths.push(path.to_string_lossy().into_owned());
        }
        Ok(result)
    }

    /// Small PNG ghost for native drag-out; the drag session uses the image's natural size.
    pub fn drag_icon(&self, expected_root: &str, asset_id: &str) -> Result<String, String> {
        use base64::Engine;
        self.check_expected_root(expected_root)?;
        let path = self.validate_selection(&[asset_id.to_string()])?.remove(0);
        let thumb = image::open(&path)
            .map_err(|e| format!("无法生成拖动预览：{e}"))?
            .thumbnail(DRAG_ICON_SIZE, DRAG_ICON_SIZE)
            .to_rgba8();
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(thumb)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
        ))
    }

    pub fn asset_paths(
        &self,
        expected_root: &str,
        ids: &[String],
        wechat_ready: bool,
    ) -> Result<Vec<String>, String> {
        self.check_expected_root(expected_root)?;
        if wechat_ready {
            Ok(self.prepare_wechat(expected_root, ids)?.paths)
        } else {
            Ok(self
                .validate_selection(ids)?
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn wechat_converts_static_and_animated_webp_caches_and_preserves_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::create(dir.path()).unwrap();
        let static_path = dir.path().join("static.webp");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 0]))
            .save(&static_path)
            .unwrap();
        let animation_path = dir.path().join("animated.webp");
        let mut animation = crate::library::tests::animated_webp();
        // Different millisecond delays exercise per-frame timing preservation.
        let mut start = 0;
        let mut duration = 120_u32;
        while let Some(offset) = animation[start..].windows(4).position(|w| w == b"ANMF") {
            let index = start + offset;
            animation[index + 20..index + 23].copy_from_slice(&duration.to_le_bytes()[..3]);
            duration = 230;
            start = index + 24;
        }
        fs::write(&animation_path, animation).unwrap();
        let png = dir.path().join("original.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]))
            .save(&png)
            .unwrap();
        lib.import_files(vec![static_path, animation_path, png], "本地".into())
            .unwrap();
        let root = lib.snapshot().root;
        let ids: Vec<_> = lib
            .snapshot()
            .items
            .iter()
            .rev()
            .map(|i| i.id.clone())
            .collect();
        let prepared = lib.prepare_wechat(&root, &ids).unwrap();
        assert_eq!(prepared.converted, 2);
        assert!(prepared.oversized.is_empty());
        assert!(prepared.paths[0].contains("/assets/"));
        assert!(prepared.paths[1].ends_with(".gif"));
        assert!(prepared.paths[2].ends_with(".png"));
        let frames =
            image::codecs::gif::GifDecoder::new(Cursor::new(fs::read(&prepared.paths[1]).unwrap()))
                .unwrap()
                .into_frames()
                .collect_frames()
                .unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].delay().numer_denom_ms(), (120, 1));
        assert_eq!(frames[1].delay().numer_denom_ms(), (230, 1));
        assert_eq!(frames[1].buffer().get_pixel(0, 0).0, [0, 0, 255, 255]);
        assert_eq!(
            image::open(&prepared.paths[2])
                .unwrap()
                .to_rgba8()
                .get_pixel(0, 0)[3],
            0
        );
        let modified = fs::metadata(&prepared.paths[1])
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            lib.prepare_wechat(&root, &ids).unwrap().paths,
            prepared.paths
        );
        assert_eq!(
            fs::metadata(&prepared.paths[1])
                .unwrap()
                .modified()
                .unwrap(),
            modified
        );
        assert!(lib.prepare_wechat("wrong-root", &ids).is_err());
        assert!(lib.prepare_wechat(&root, &["f".repeat(64)]).is_err());
        assert!(lib.prepare_wechat(&root, &[]).is_err());
        let backup = lib.backup_to(dir.path()).unwrap();
        assert!(!Path::new(&backup.path).join("cache").exists());
        let before_scan = lib
            .scan_static_duplicates(&[], &std::collections::HashSet::new())
            .unwrap();
        fs::write(
            Path::new(&root).join("cache/wechat/not-a-real-image.webp"),
            b"bad cache",
        )
        .unwrap();
        let after_scan = lib
            .scan_static_duplicates(&[], &std::collections::HashSet::new())
            .unwrap();
        assert_eq!(
            serde_json::to_value(before_scan).unwrap(),
            serde_json::to_value(after_scan).unwrap()
        );
        let report = lib
            .import_files(vec![PathBuf::from(&root).join("cache")], "本地".into())
            .unwrap();
        assert_eq!(report.added + report.duplicates, 0);
    }

    #[test]
    fn animated_conversion_preserves_transparent_frames_and_infinite_loop() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::create(dir.path()).unwrap();
        let input = dir.path().join("transparent.webp");
        fs::write(
            &input,
            crate::library::tests::animated_webp_colors(&[[255, 0, 0, 255], [0, 0, 0, 0]]),
        )
        .unwrap();
        lib.import_files(vec![input], "本地".into()).unwrap();
        let root = lib.snapshot().root;
        let id = lib.snapshot().items[0].id.clone();
        let prepared = lib.prepare_wechat(&root, &[id]).unwrap();
        let bytes = fs::read(&prepared.paths[0]).unwrap();
        assert!(bytes
            .windows(19)
            .any(|w| w == b"NETSCAPE2.0\x03\x01\x00\x00\x00\x21\xf9\x04"));
        let frames = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
            .unwrap()
            .into_frames()
            .collect_frames()
            .unwrap();
        assert_eq!(frames[0].buffer().get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(frames[1].buffer().get_pixel(0, 0)[3], 0);
    }

    #[test]
    fn wechat_reports_large_originals_without_changing_them() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::create(dir.path()).unwrap();
        let path = dir.path().join("large.png");
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([0, 0, 0, 255]),
        ))
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
        bytes.resize(1024 * 1024 + 1, 0);
        fs::write(&path, &bytes).unwrap();
        lib.import_files(vec![path], "本地".into()).unwrap();
        let root = lib.snapshot().root;
        let id = lib.snapshot().items[0].id.clone();
        let prepared = lib.prepare_wechat(&root, &[id.clone()]).unwrap();
        assert_eq!(prepared.oversized, vec![id]);
        assert_eq!(prepared.converted, 0);
        assert_eq!(fs::read(&prepared.paths[0]).unwrap(), bytes);
    }
}

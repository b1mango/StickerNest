//! Small static previews for the grid: `<root>/cache/thumbs/<id>.png`, first
//! frame, longest edge ≤ 256 px. Originals stay untouched; thumbnails are a
//! disposable cache (not backed up) and are regenerated when missing.
use serde::Serialize;
use std::{
    collections::HashSet,
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

pub const MAX_EDGE: u32 = 256;
const WORKERS: usize = 4;

#[derive(Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThumbReport {
    pub created: usize,
    pub skipped: usize,
    pub failed: usize,
    pub total: usize,
}

pub fn thumbs_dir(root: &Path) -> PathBuf {
    root.join("cache").join("thumbs")
}

fn is_thumb_name(name: &str) -> bool {
    name.strip_suffix(".png").is_some_and(|id| {
        id.len() == 64 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Decodes the first frame of `source` and writes a ≤256 px PNG atomically.
pub fn make_thumbnail(source: &Path, target: &Path) -> Result<(), String> {
    let bytes = fs::read(source).map_err(|e| e.to_string())?;
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?;
    let image = if image.width() > MAX_EDGE || image.height() > MAX_EDGE {
        image.thumbnail(MAX_EDGE, MAX_EDGE)
    } else {
        image
    };
    let mut png = Vec::new();
    image.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    let dir = target.parent().ok_or("缩略图路径无效")?;
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    temp.write_all(&png).map_err(|e| e.to_string())?;
    temp.persist(target).map_err(|e| e.error.to_string())?;
    Ok(())
}

/// Generates missing thumbnails for `(id, original)` pairs and removes thumbnails
/// of assets no longer in the library. `on_progress(done, total)` is called after
/// every item from worker threads.
pub fn ensure(
    root: &Path,
    items: &[(String, PathBuf)],
    on_progress: impl Fn(usize, usize) + Sync,
) -> Result<ThumbReport, String> {
    let dir = thumbs_dir(root);
    if fs::symlink_metadata(&dir).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("缩略图目录不能是符号链接".into());
    }
    fs::create_dir_all(&dir).map_err(|e| format!("无法创建缩略图目录：{e}"))?;

    let wanted: HashSet<String> = items.iter().map(|(id, _)| format!("{id}.png")).collect();
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_thumb_name(&name) && !wanted.contains(&name) {
            let _ = fs::remove_file(entry.path());
        }
    }

    let total = items.len();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let report = Mutex::new(ThumbReport { total, ..Default::default() });
    std::thread::scope(|scope| {
        for _ in 0..WORKERS.min(total.max(1)) {
            scope.spawn(|| loop {
                let at = next.fetch_add(1, Ordering::Relaxed);
                let Some((id, source)) = items.get(at) else { break };
                let target = dir.join(format!("{id}.png"));
                let outcome = if fs::metadata(&target).is_ok_and(|m| m.len() > 0) {
                    Ok(false)
                } else {
                    make_thumbnail(source, &target).map(|_| true)
                };
                {
                    let mut report = report.lock().unwrap_or_else(|e| e.into_inner());
                    match outcome {
                        Ok(true) => report.created += 1,
                        Ok(false) => report.skipped += 1,
                        Err(_) => report.failed += 1,
                    }
                }
                on_progress(done.fetch_add(1, Ordering::Relaxed) + 1, total);
            });
        }
    });
    Ok(report.into_inner().unwrap_or_else(|e| e.into_inner()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::animated_webp;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    const D: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

    #[test]
    fn thumbnails_shrink_keep_alpha_take_first_frame_and_clean_orphans() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let big = root.join("big.png");
        image::RgbaImage::from_pixel(1024, 512, image::Rgba([10, 20, 30, 0])).save(&big).unwrap();
        let small = root.join("small.png");
        image::RgbaImage::from_pixel(40, 30, image::Rgba([1, 2, 3, 255])).save(&small).unwrap();
        let animated = root.join("anim.webp");
        fs::write(&animated, animated_webp()).unwrap();
        let broken = root.join("broken.gif");
        fs::write(&broken, b"GIF89a-not-really").unwrap();

        let thumbs = thumbs_dir(root);
        fs::create_dir_all(&thumbs).unwrap();
        fs::write(thumbs.join(format!("{}.png", "e".repeat(64))), b"orphan").unwrap();
        fs::write(thumbs.join("notes.txt"), b"keep").unwrap();

        let items = vec![(A.into(), big), (B.into(), small), (C.into(), animated), (D.into(), broken)];
        let calls = AtomicUsize::new(0);
        let report = ensure(root, &items, |_, total| { assert_eq!(total, 4); calls.fetch_add(1, Ordering::Relaxed); }).unwrap();
        assert_eq!(report, ThumbReport { created: 3, skipped: 0, failed: 1, total: 4 });
        assert_eq!(calls.load(Ordering::Relaxed), 4);

        let a = image::open(thumbs.join(format!("{A}.png"))).unwrap().to_rgba8();
        assert_eq!(a.dimensions(), (256, 128));
        assert_eq!(a.get_pixel(0, 0).0[3], 0, "alpha is preserved");
        assert_eq!(image::open(thumbs.join(format!("{B}.png"))).unwrap().to_rgba8().dimensions(), (40, 30));
        // First frame of the fixture animation is red.
        assert_eq!(image::open(thumbs.join(format!("{C}.png"))).unwrap().to_rgba8().get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert!(!thumbs.join(format!("{}.png", "e".repeat(64))).exists());
        assert!(thumbs.join("notes.txt").exists());

        let again = ensure(root, &items, |_, _| {}).unwrap();
        assert_eq!(again, ThumbReport { created: 0, skipped: 3, failed: 1, total: 4 });
    }
}

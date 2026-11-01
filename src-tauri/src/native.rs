use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use objc2::{rc::autoreleasepool, runtime::ProtocolObject};
    use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting, NSWorkspace};
    use objc2_foundation::{NSArray, NSData, NSFileManager, NSString, NSURL};

    fn url(path: &Path) -> objc2::rc::Retained<NSURL> {
        NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
    }
    pub fn is_package(path: &Path) -> Result<bool, String> {
        Ok(autoreleasepool(|_| {
            NSWorkspace::sharedWorkspace()
                .isFilePackageAtPath(&NSString::from_str(&path.to_string_lossy()))
        }))
    }
    pub fn reveal(paths: &[PathBuf]) -> Result<(), String> {
        autoreleasepool(|_| {
            let urls: Vec<_> = paths.iter().map(|p| url(p)).collect();
            NSWorkspace::sharedWorkspace()
                .activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&urls));
        });
        Ok(())
    }
    pub fn trash(path: &Path) -> Result<(), String> {
        autoreleasepool(|_| {
            NSFileManager::defaultManager()
                .trashItemAtURL_resultingItemURL_error(&url(path), None)
                .map_err(|e| format!("无法移入系统废纸篓：{e}"))
        })
    }
    pub fn copy(paths: &[String]) -> Result<(), String> {
        autoreleasepool(|_| {
            let mut items = Vec::new();
            for path in paths {
                let item = NSPasteboardItem::new();
                let file_url = url(Path::new(path))
                    .absoluteString()
                    .ok_or("无法生成文件 URL")?;
                if !item.setString_forType(&file_url, &NSString::from_str("public.file-url")) {
                    return Err("无法写入剪贴板文件 URL".into());
                }
                if paths.len() == 1 {
                    let ext = Path::new(path)
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    let image_type = match ext {
                        "gif" => Some("com.compuserve.gif"),
                        "png" => Some("public.png"),
                        "jpg" | "jpeg" => Some("public.jpeg"),
                        "webp" => Some("public.png"),
                        _ => None,
                    };
                    if let Some(image_type) = image_type {
                        let bytes = if ext == "webp" {
                            let mut output = std::io::Cursor::new(Vec::new());
                            image::open(path)
                                .map_err(|e| format!("无法读取剪贴板图片：{e}"))?
                                .write_to(&mut output, image::ImageFormat::Png)
                                .map_err(|e| e.to_string())?;
                            output.into_inner()
                        } else {
                            std::fs::read(path).map_err(|e| format!("无法读取剪贴板图片：{e}"))?
                        };
                        let data = NSData::with_bytes(&bytes);
                        if !item.setData_forType(&data, &NSString::from_str(image_type)) {
                            return Err("无法写入剪贴板图片".into());
                        }
                    }
                }
                items.push(item);
            }
            let writers: Vec<&ProtocolObject<dyn NSPasteboardWriting>> = items
                .iter()
                .map(|item| ProtocolObject::from_ref(&**item))
                .collect();
            let pasteboard = NSPasteboard::generalPasteboard();
            pasteboard.clearContents();
            if !pasteboard.writeObjects(&NSArray::from_slice(&writers)) {
                return Err("系统剪贴板写入失败".into());
            }
            Ok(())
        })
    }
}
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(target_os = "macos"))]
pub fn reveal(_: &[PathBuf]) -> Result<(), String> {
    Err("仅支持 macOS 访达".into())
}
#[cfg(not(target_os = "macos"))]
pub fn trash(_: &Path) -> Result<(), String> {
    Err("仅支持 macOS 系统废纸篓".into())
}
#[cfg(not(target_os = "macos"))]
pub fn copy(_: &[String]) -> Result<(), String> {
    Err("仅支持 macOS 剪贴板".into())
}

pub mod backup;
pub mod bookmark;
pub mod classify;
pub mod collection_runtime;
pub mod library;
pub mod management;
pub mod similar;
pub mod sharing;
pub mod thumbs;
pub mod vision;
mod native;
mod preferences;
pub mod wechat;
pub mod wechat_db;
use management::ManagementSnapshot;

use backup::BackupSummary;
use library::{ImportReport, Library, LibrarySnapshot};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{Emitter, Manager, State};

#[derive(Default)]
struct LibraryState(Mutex<Option<Library>>);

fn authorize_assets(app: &tauri::AppHandle, library: &Library) -> Vec<String> {
    let scope = app.asset_protocol_scope();
    let mut warnings = Vec::new();
    for item in library.snapshot().items {
        let result = library
            .asset_path(&item.id)
            .and_then(|path| scope.allow_file(path).map_err(|e| e.to_string()));
        if let Err(error) = result {
            warnings.push(format!("{}：{}", item.name, error));
        }
    }
    warnings
}

#[tauri::command]
async fn select_library(
    app: tauri::AppHandle,
    path: String,
    create: bool,
) -> Result<LibrarySnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LibraryState>();
        let mut current = state.0.lock().map_err(|_| "资料库忙，请重新打开应用")?;
        if !create {
            if let Some(library) = current.as_ref() {
                let picked = match Library::locate(Path::new(&path)) {
                    Ok(library::FolderKind::Library(root)) => Some(root),
                    _ => None,
                };
                if picked.as_ref() == Some(&PathBuf::from(library.snapshot().root)) {
                    remember_library(&app, &library.snapshot().root);
                    return Ok(library.snapshot());
                }
            }
        }
        let library = if create {
            Library::create_at(&PathBuf::from(path))?
        } else {
            Library::open(&PathBuf::from(path))?
        };
        let warnings = authorize_assets(&app, &library);
        if !warnings.is_empty() {
            return Err(format!("资料库预览授权失败：{}", warnings.join("；")));
        }
        let snapshot = library.snapshot();
        remember_library(&app, &snapshot.root);
        // A failed selection leaves the currently open library untouched.
        *current = Some(library);
        Ok(snapshot)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderInfo {
    kind: &'static str,
    root: String,
}

/// Tells the picker what the chosen folder is before opening it.
#[tauri::command]
fn inspect_library_folder(path: String) -> Result<FolderInfo, String> {
    let (kind, root) = match Library::locate(Path::new(&path))? {
        library::FolderKind::Library(root) => ("library", root),
        library::FolderKind::Empty(root) => ("empty", root),
        library::FolderKind::Other(root) => ("other", root),
    };
    Ok(FolderInfo { kind, root: root.to_string_lossy().into_owned() })
}

#[derive(Clone, Serialize)]
struct ThumbProgress {
    done: usize,
    total: usize,
}

/// Generates grid thumbnails without holding the library lock for the whole run.
#[tauri::command]
async fn ensure_thumbnails(app: tauri::AppHandle, expected_root: String) -> Result<thumbs::ThumbReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (root, items) = {
            let state = app.state::<LibraryState>();
            let current = state.0.lock().map_err(|_| "资料库忙，请稍后再试")?;
            let library = current.as_ref().ok_or("请先打开资料库")?;
            let snapshot = library.snapshot();
            if snapshot.root != expected_root {
                return Err("资料库已切换，请重试".to_string());
            }
            let items = snapshot.items.iter()
                .map(|item| Ok((item.id.clone(), library.asset_path(&item.id)?)))
                .collect::<Result<Vec<_>, String>>()?;
            (PathBuf::from(snapshot.root), items)
        };
        let dir = thumbs::thumbs_dir(&root);
        std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建缩略图目录：{e}"))?;
        app.asset_protocol_scope().allow_directory(&dir, false).map_err(|e| e.to_string())?;
        thumbs::ensure(&root, &items, |done, total| {
            if done % 20 == 0 || done == total {
                let _ = app.emit("thumbs-progress", ThumbProgress { done, total });
            }
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn recent_libraries(app: tauri::AppHandle) -> Vec<String> {
    app.path().app_config_dir().map(|config| preferences::recent(&config)).unwrap_or_default()
}

#[tauri::command]
fn current_library(state: State<'_, LibraryState>) -> Result<Option<LibrarySnapshot>, String> {
    Ok(state
        .0
        .lock()
        .map_err(|_| "资料库忙，请重新打开应用")?
        .as_ref()
        .map(Library::snapshot))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResponse {
    snapshot: LibrarySnapshot,
    report: ImportReport,
    preview_warnings: Vec<String>,
}

#[tauri::command]
async fn import_images(
    app: tauri::AppHandle,
    paths: Vec<String>,
    source: String,
) -> Result<ImportResponse, String> {
    if paths.len() > 2000 {
        return Err("单次最多选择 2000 个文件，请分批导入".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LibraryState>();
        let mut current = state.0.lock().map_err(|_| "资料库忙，请重新打开应用")?;
        let library = current.as_mut().ok_or("请先创建或打开资料库")?;
        let report =
            library.import_files(paths.into_iter().map(PathBuf::from).collect(), source)?;
        let preview_warnings = authorize_assets(&app, library);
        Ok(ImportResponse {
            snapshot: library.snapshot(),
            report,
            preview_warnings,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_management(state: State<'_, LibraryState>) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current.as_ref().ok_or("请先打开资料库")?.get_management()
}
#[tauri::command]
fn save_metadata(
    state: State<'_, LibraryState>,
    expected_root: String,
    asset_id: String,
    name: String,
    tags: Vec<String>,
    collections: Vec<String>,
) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current.as_ref().ok_or("请先打开资料库")?.save_metadata(
        &expected_root,
        asset_id,
        name,
        tags,
        collections,
    )
}
#[tauri::command]
fn batch_rename(
    state: State<'_, LibraryState>,
    expected_root: String,
    asset_ids: Vec<String>,
    prefix: String,
    start: u32,
) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current.as_ref().ok_or("请先打开资料库")?.batch_rename(
        &expected_root,
        asset_ids,
        prefix,
        start,
    )
}
#[tauri::command]
fn batch_labels(
    state: State<'_, LibraryState>,
    expected_root: String,
    asset_ids: Vec<String>,
    add_tags: Vec<String>,
    remove_tags: Vec<String>,
    add_collections: Vec<String>,
    remove_collections: Vec<String>,
) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current.as_ref().ok_or("请先打开资料库")?.batch_labels(
        &expected_root,
        asset_ids,
        add_tags,
        remove_tags,
        add_collections,
        remove_collections,
    )
}
#[tauri::command]
fn import_provenance(
    state: State<'_, LibraryState>,
    expected_root: String,
    path: String,
    account_alias: String,
    account_id: Option<String>,
    platform: Option<String>,
) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current.as_ref().ok_or("请先打开资料库")?.import_provenance(
        &expected_root,
        &PathBuf::from(path),
        account_alias,
        account_id,
        platform,
    )
}
#[tauri::command]
fn add_collection(
    state: State<'_, LibraryState>,
    expected_root: String,
    name: String,
    asset_ids: Vec<String>,
) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current
        .as_ref()
        .ok_or("请先打开资料库")?
        .add_collection_name(&expected_root, name, asset_ids)
}
#[tauri::command]
fn set_trash(
    state: State<'_, LibraryState>,
    expected_root: String,
    asset_ids: Vec<String>,
    trashed: bool,
) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current
        .as_ref()
        .ok_or("请先打开资料库")?
        .set_trash(&expected_root, asset_ids, trashed)
}
#[tauri::command]
async fn backup_library(
    app: tauri::AppHandle,
    target_parent: String,
) -> Result<BackupSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LibraryState>();
        let current = state.0.lock().map_err(|_| "资料库忙")?;
        current
            .as_ref()
            .ok_or("请先打开资料库")?
            .backup_to(Path::new(&target_parent))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn restore_backup(app: tauri::AppHandle, backup_dir: String, target_parent: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = backup::restore(&PathBuf::from(backup_dir), &PathBuf::from(target_parent))?;
        let root = path.to_string_lossy().into_owned();
        remember_library(&app, &root);
        Ok(root)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn scan_duplicates(app: tauri::AppHandle) -> Result<similar::ScanReport, String> {
    // Long scans run off the main thread so the UI stays responsive; the
    // foreground library keeps its lock alive while the clone decodes assets.
    let library = {
        let state = app.state::<LibraryState>();
        let current = state.0.lock().map_err(|_| "资料库忙")?;
        current.as_ref().ok_or("请先打开资料库")?.clone()
    };
    tauri::async_runtime::spawn_blocking(move || {
        let management = library.get_management()?;
        let grouped: std::collections::HashSet<String> = management
            .groups
            .iter()
            .flat_map(|g| g.member_ids.clone())
            .collect();
        library.scan_static_duplicates(&management.ignored_pairs, &grouped)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn create_group(
    state: State<'_, LibraryState>,
    expected_root: String,
    member_ids: Vec<String>,
    main_asset_id: String,
    tags: Vec<String>,
    collections: Vec<String>,
) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current.as_ref().ok_or("请先打开资料库")?.create_group(
        &expected_root,
        member_ids,
        main_asset_id,
        tags,
        collections,
    )
}
#[tauri::command]
fn save_group_metadata(
    state: State<'_, LibraryState>,
    expected_root: String,
    group_id: String,
    tags: Vec<String>,
    collections: Vec<String>,
) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current.as_ref().ok_or("请先打开资料库")?.save_group_metadata(
        &expected_root,
        &group_id,
        tags,
        collections,
    )
}
#[tauri::command]
fn disband_group(
    state: State<'_, LibraryState>,
    expected_root: String,
    group_id: String,
) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current
        .as_ref()
        .ok_or("请先打开资料库")?
        .disband_group(&expected_root, &group_id)
}
#[tauri::command]
fn ignore_pair(
    state: State<'_, LibraryState>,
    expected_root: String,
    asset_a: String,
    asset_b: String,
) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current
        .as_ref()
        .ok_or("请先打开资料库")?
        .ignore_pair(&expected_root, &asset_a, &asset_b)
}
#[tauri::command]
async fn collect_douyin_fetch(app: tauri::AppHandle) -> Result<backup::CollectStageResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = {
            let state = app.state::<LibraryState>();
            let current = state.0.lock().map_err(|_| "资料库忙，请稍后再试")?;
            PathBuf::from(current.as_ref().ok_or("请先打开资料库")?.snapshot().root)
        };
        let runtime = collection_runtime(&app)?;
        let profile = app.path().app_data_dir().map_err(|e| e.to_string())?.join("douyin-browser");
        std::fs::create_dir_all(&profile).map_err(|e| format!("无法创建专用浏览器目录：{e}"))?;
        backup::collect_douyin_fetch(&root, &runtime, &profile, |stage, message, done, total| {
            app.emit("douyin-progress", DouyinProgress { stage, message, done, total })
                .map_err(|e| e.to_string())
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DouyinProgress {
    stage: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    done: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<u64>,
}

#[cfg(test)]
#[test]
fn douyin_progress_serializes_optional_counts() {
    let without_counts = DouyinProgress {
        stage: "launching".into(), message: "正在启动".into(), done: None, total: None,
    };
    assert_eq!(serde_json::to_value(without_counts).unwrap(), serde_json::json!({
        "stage": "launching", "message": "正在启动",
    }));
    let with_counts = DouyinProgress {
        stage: "importing".into(), message: "已导入 5/6".into(), done: Some(5), total: Some(6),
    };
    assert_eq!(serde_json::to_value(with_counts).unwrap(), serde_json::json!({
        "stage": "importing", "message": "已导入 5/6", "done": 5, "total": 6,
    }));
}

#[tauri::command]
async fn collect_douyin_import(app: tauri::AppHandle) -> Result<CollectImportResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LibraryState>();
        let mut current = state.0.lock().map_err(|_| "资料库忙，请稍后再试")?;
        let library = current.as_mut().ok_or("请先打开资料库")?;
        let mut progress_error = None;
        let (report, report_path) = library.collect_douyin_import(|done, total| {
            if done % 5 == 0 || done == total {
                if let Err(error) = app.emit("douyin-progress", DouyinProgress {
                    stage: "importing".into(),
                    message: format!("已导入 {done}/{total}"),
                    done: Some(done as u64),
                    total: Some(total as u64),
                }) {
                    if progress_error.is_none() {
                        progress_error = Some(error.to_string());
                    }
                }
            }
        })?;
        if let Some(error) = progress_error {
            return Err(format!("发送导入进度失败：{error}"));
        }
        let preview_warnings = authorize_assets(&app, library);
        Ok(CollectImportResponse {
            snapshot: library.snapshot(),
            report,
            preview_warnings,
            report_path: report_path.to_string_lossy().into_owned(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

fn collection_runtime(app: &tauri::AppHandle) -> Result<collection_runtime::CollectionRuntime, String> {
    let root = app.path().resource_dir().map_err(|e| e.to_string())?.join("collection-runtime");
    // A debug .app uses its bundled resources too; only tauri dev needs the
    // build-prepared runtime beside the project.
    #[cfg(debug_assertions)]
    let root = if root.is_dir() { root } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../output/collection-runtime")
    };
    collection_runtime::CollectionRuntime::at(root)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CollectImportResponse {
    snapshot: LibrarySnapshot,
    report: ImportReport,
    preview_warnings: Vec<String>,
    report_path: String,
}

#[tauri::command]
async fn import_wechat_manifest(
    app: tauri::AppHandle,
    manifest_path: String,
) -> Result<backup::CollectStageResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LibraryState>();
        let current = state.0.lock().map_err(|_| "资料库忙，请稍后再试")?;
        let runtime = collection_runtime(&app)?;
        current
            .as_ref()
            .ok_or("请先打开资料库")?
            .import_wechat_manifest(&runtime, Path::new(&manifest_path))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn wechat_detect_accounts() -> Result<Vec<wechat::WeChatAccount>, String> {
    wechat::detect_accounts()
}

#[tauri::command]
fn wechat_check_running(app: tauri::AppHandle) -> usize {
    let _ = app;
    match wechat::check_wechat_running() {
        wechat::WeChatRunning::NotRunning => 0,
        wechat::WeChatRunning::Running(n) => n,
    }
}

#[tauri::command]
fn wechat_grant_bookmark(path: String) -> Result<String, String> {
    crate::bookmark::store_bookmark(&path)?;
    Ok(path)
}

#[tauri::command]
async fn wechat_dump_and_export(app: tauri::AppHandle, wxid: String) -> Result<WeChatUrlsResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let urls = wechat_db_urls(&app, &wxid)?;
        Ok(urls)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WeChatUrlsResult {
    urls_txt: String,
    count: usize,
    stage_detail: String,
    used_cached_key: bool,
}

fn wechat_db_urls(app: &tauri::AppHandle, wxid: &str) -> Result<WeChatUrlsResult, String> {
    let root = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| "无法定位用户主目录".to_string())?
        .join("Library/Containers/com.tencent.xinWeChat/Data/Documents/xwechat_files")
        .join(wxid);
    let db_path = root.join("db_storage/emoticon/emoticon.db");
    if !db_path.is_file() {
        return Err("该账号没有找到表情数据库".into());
    }
    let library_root = {
        let state = app.state::<LibraryState>();
        let current = state.0.lock().map_err(|_| "资料库忙")?;
        current
            .as_ref()
            .ok_or("请先打开资料库")?
            .snapshot()
            .root
    };
    let stage_dir = Path::new(&library_root).join("downloads/wechat");
    std::fs::create_dir_all(&stage_dir).map_err(|e| e.to_string())?;
    let urls_path = stage_dir.join("emoticon_urls.txt");

    let (candidates, used_cached) = match wechat::cached_key(wxid)? {
        Some(candidates) => (candidates, true),
        None => {
            let candidates = wechat::dump_key(wxid, &mut |_note| {})?;
            (candidates, false)
        }
    };
    // The dump may have landed on another account's emoticon.db; resolve the
    // owner of these candidates before decrypting (clone-login wxid may differ).
    let owner = match wechat::resolve_candidate_account(&candidates) {
        Ok(owner) => owner,
        Err(_) => wxid.to_string(),
    };
    let effective_db = root
        .parent()
        .ok_or("微信数据目录结构异常")?
        .join(&owner)
        .join("db_storage/emoticon/emoticon.db");
    let db_path = if effective_db.is_file() { effective_db } else { db_path };
    let count = match wechat_db::export_urls(&db_path, &candidates, &urls_path) {
        Ok(count) => count,
        Err(e) if used_cached => {
            // Cached candidates no longer decrypt: refresh them once, then retry.
            let fresh = wechat::dump_key(wxid, &mut |_note| {})?;
            match wechat_db::export_urls(&db_path, &fresh, &urls_path) {
                Ok(count) => count,
                Err(e2) => return Err(e2),
            }
        }
        Err(e) => return Err(e),
    };
    let _ = app;
    Ok(WeChatUrlsResult {
        urls_txt: urls_path.to_string_lossy().into_owned(),
        count,
        stage_detail: if used_cached {
            "使用缓存密钥本地解密，未打扰微信".into()
        } else {
            "已通过微信本地副本取得密钥并完成解密，密钥已缓存：后续无需再打扰微信".into()
        },
        used_cached_key: used_cached,
    })
}

#[tauri::command]
async fn collect_wechat_import(app: tauri::AppHandle) -> Result<CollectImportResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LibraryState>();
        let mut current = state.0.lock().map_err(|_| "资料库忙，请稍后再试")?;
        let library = current.as_mut().ok_or("请先打开资料库")?;
        let (report, report_path) = library.collect_wechat_import()?;
        let preview_warnings = authorize_assets(&app, library);
        Ok(CollectImportResponse {
            snapshot: library.snapshot(),
            report,
            preview_warnings,
            report_path: report_path.to_string_lossy().into_owned(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn export_assets(
    state: State<'_, LibraryState>,
    expected_root: String,
    asset_ids: Vec<String>,
    target_parent: String,
    name_map: std::collections::HashMap<String, String>,
) -> Result<backup::ExportSummary, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    let library = current.as_ref().ok_or("请先打开资料库")?;
    library.check_expected_root(&expected_root)?;
    library.export_assets(Path::new(&target_parent), &asset_ids, &name_map)
}

/// Best effort: failing to remember the library must not fail opening it.
fn remember_library(app: &tauri::AppHandle, root: &str) {
    if let Ok(config) = app.path().app_config_dir() {
        if let Err(error) = preferences::remember(&config, root) {
            eprintln!("无法记住资料库位置：{error}");
        }
    }
}

#[tauri::command]
async fn drag_icon(app: tauri::AppHandle, expected_root: String, asset_id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LibraryState>();
        let current = state.0.lock().map_err(|_| "资料库忙")?;
        current.as_ref().ok_or("请先打开资料库")?.drag_icon(&expected_root, &asset_id)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn reveal_in_finder(app: tauri::AppHandle, expected_root: String, asset_ids: Vec<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LibraryState>();
        let current = state.0.lock().map_err(|_| "资料库忙")?;
        let library = current.as_ref().ok_or("请先打开资料库")?;
        library.check_expected_root(&expected_root)?;
        let paths = if asset_ids.is_empty() { vec![PathBuf::from(expected_root)] }
            else { library.validate_selection(&asset_ids)? };
        native::reveal(&paths)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn set_favorite(state: State<'_, LibraryState>, expected_root: String, asset_ids: Vec<String>, favorite: bool) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current.as_ref().ok_or("请先打开资料库")?.set_favorite(&expected_root, asset_ids, favorite)
}
#[tauri::command]
fn mark_used(state: State<'_, LibraryState>, expected_root: String, asset_ids: Vec<String>) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current.as_ref().ok_or("请先打开资料库")?.mark_used(&expected_root, asset_ids)
}
#[tauri::command]
fn rename_collection(state: State<'_, LibraryState>, expected_root: String, from: String, to: String) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current.as_ref().ok_or("请先打开资料库")?.rename_collection(&expected_root, from, to)
}
#[tauri::command]
fn delete_collection(state: State<'_, LibraryState>, expected_root: String, name: String) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current.as_ref().ok_or("请先打开资料库")?.delete_collection(&expected_root, name)
}
#[tauri::command]
async fn prepare_wechat(app: tauri::AppHandle, expected_root: String, asset_ids: Vec<String>) -> Result<sharing::PreparedWechat, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LibraryState>();
        let current = state.0.lock().map_err(|_| "资料库忙")?;
        current.as_ref().ok_or("请先打开资料库")?.prepare_wechat(&expected_root, &asset_ids)
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
async fn asset_paths(app: tauri::AppHandle, expected_root: String, asset_ids: Vec<String>, wechat_ready: bool) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LibraryState>();
        let current = state.0.lock().map_err(|_| "资料库忙")?;
        current.as_ref().ok_or("请先打开资料库")?.asset_paths(&expected_root, &asset_ids, wechat_ready)
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
async fn copy_to_clipboard(app: tauri::AppHandle, expected_root: String, asset_ids: Vec<String>, wechat_ready: bool) -> Result<sharing::ClipboardResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LibraryState>();
        let current = state.0.lock().map_err(|_| "资料库忙")?;
        let library = current.as_ref().ok_or("请先打开资料库")?;
        let prepared = if wechat_ready { library.prepare_wechat(&expected_root, &asset_ids)? }
            else { sharing::PreparedWechat { paths: library.asset_paths(&expected_root, &asset_ids, false)?, converted: 0, oversized: vec![] } };
        native::copy(&prepared.paths)?;
        library.mark_used(&expected_root, asset_ids)?;
        Ok(sharing::ClipboardResult { count: prepared.paths.len(), converted: prepared.converted, oversized: prepared.oversized })
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
async fn delete_permanently(app: tauri::AppHandle, expected_root: String, asset_ids: Vec<String>) -> Result<library::DeleteResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LibraryState>();
        let mut current = state.0.lock().map_err(|_| "资料库忙")?;
        current.as_mut().ok_or("请先打开资料库")?.delete_permanently(&expected_root, asset_ids)
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
async fn empty_trash(app: tauri::AppHandle, expected_root: String) -> Result<library::DeleteResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LibraryState>();
        let mut current = state.0.lock().map_err(|_| "资料库忙")?;
        current.as_mut().ok_or("请先打开资料库")?.empty_trash(&expected_root)
    }).await.map_err(|e| e.to_string())?
}

/// Bundled Apple Vision OCR helper (tools/ocr-helper); tauri dev uses the project copy.
fn ocr_helper(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let bundled = app.path().resource_dir().map_err(|e| e.to_string())?.join("ocr-helper");
    if bundled.is_file() {
        return Ok(bundled);
    }
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/ocr-helper/ocr-helper");
    if cfg!(debug_assertions) && dev.is_file() { Ok(dev) } else { Err("应用缺少文字识别组件，请重新安装完整应用".into()) }
}

/// Runs the OCR helper on `paths`, returning recognized text in the same order.
fn recognize_text(helper: &Path, paths: &[PathBuf]) -> Result<Vec<String>, String> {
    let output = std::process::Command::new(helper).args(paths).output()
        .map_err(|e| format!("无法运行文字识别：{e}"))?;
    if !output.status.success() {
        return Err(format!("文字识别失败：{}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    String::from_utf8_lossy(&output.stdout).lines().map(|line| {
        let value: serde_json::Value = serde_json::from_str(line).map_err(|e| format!("文字识别输出无效：{e}"))?;
        Ok(value["text"].as_str().unwrap_or_default().to_string())
    }).collect()
}

fn classifier_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("classifier"))
}

#[tauri::command]
fn classifier_status(app: tauri::AppHandle) -> Result<classify::ClassifierStatus, String> {
    // Read-only and size-checked, so it must not wait on a long download holding the lock
    // (sync commands run on the main thread).
    classify::status_at(&classifier_dir(&app)?)
}

#[tauri::command]
async fn enable_classifier(app: tauri::AppHandle) -> Result<classify::ClassifierStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = classify::operation_lock().lock().map_err(|_| "形象识别忙，请稍后再试")?;
        let directory = classifier_dir(&app)?;
        let runtime = collection_runtime(&app)?;
        classify::download_model(&runtime, &directory, |progress| {
            app.emit("classifier-progress", progress).map_err(|e| e.to_string())
        })?;
        vision::clear_model_cache()?;
        let status = classify::status_at(&directory)?;
        if !status.enabled {
            return Err("模型下载完成但文件校验状态无效，请重试".into());
        }
        Ok(status)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn disable_classifier(app: tauri::AppHandle) -> Result<(), String> {
    let _operation = classify::operation_lock().lock().map_err(|_| "形象识别忙，请稍后再试")?;
    vision::clear_model_cache()?;
    match std::fs::remove_dir_all(classifier_dir(&app)?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法删除形象识别模型：{error}")),
    }
}

#[derive(Clone, Serialize)]
struct ClassifyProgress {
    done: usize,
    total: usize,
}

#[tauri::command]
async fn classify_library(
    app: tauri::AppHandle,
    expected_root: String,
) -> Result<classify::ClassifyResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = classify::operation_lock().lock().map_err(|_| "形象识别忙，请稍后再试")?;
        let directory = classifier_dir(&app)?;
        let status = classify::status_at(&directory)?;
        if !status.enabled {
            return Err("请先在设置中启用形象识别".into());
        }
        let model = status.model.ok_or("模型清单缺少型号")?;
        let pack = classify::CharacterPack::from_json(
            &std::fs::read(directory.join("characters.json")).map_err(|e| e.to_string())?,
        )?;
        let (root, items, originals, management) = {
            let state = app.state::<LibraryState>();
            let current = state.0.lock().map_err(|_| "资料库忙，请稍后再试")?;
            let library = current.as_ref().ok_or("请先打开资料库")?;
            library.check_expected_root(&expected_root)?;
            let snapshot = library.snapshot();
            let root = PathBuf::from(&snapshot.root);
            let mut originals = Vec::with_capacity(snapshot.items.len());
            let items = snapshot.items.iter().map(|item| {
                let original = library.asset_path(&item.id)?;
                originals.push((item.id.clone(), original.clone()));
                let thumbnail = thumbs::thumbs_dir(&root).join(format!("{}.png", item.id));
                let input = if thumbnail.is_file() { thumbnail } else { original };
                Ok((item.id.clone(), input))
            }).collect::<Result<Vec<_>, String>>()?;
            let management = library.get_management()?;
            (root, items, originals, management)
        };
        let embeddings = vision::with_cached_model(&directory.join("vision.safetensors"), |vision_model| {
            classify::update_embeddings(&root, &model, &items, |paths| {
                vision_model.embed_paths(paths)
            }, |done, total| {
                let _ = app.emit("classify-progress", ClassifyProgress { done, total });
            })
        })?;
        // Text rules (e.g. 龙图 = 熊猫头 with 龙 in the caption) need on-device OCR, only
        // for assets that look like a rule's base character; results are cached per asset.
        let originals: BTreeMap<&str, &PathBuf> = originals.iter().map(|(id, path)| (id.as_str(), path)).collect();
        let wanted: Vec<(String, PathBuf)> = classify::text_candidates(&pack, &embeddings, &management)
            .into_iter()
            .filter_map(|id| originals.get(id.as_str()).map(|path| (id.clone(), (*path).clone())))
            .collect();
        let texts = if wanted.is_empty() { classify::load_texts(&root) } else {
            let helper = ocr_helper(&app)?;
            classify::update_texts(&root, &wanted, |paths| recognize_text(&helper, paths))?
        };
        let dismissed = classify::load_dismissed(&root)?;
        let suggestions = classify::score_embeddings(&pack, &embeddings, &management, &dismissed, &texts);
        Ok(classify::ClassifyResponse { characters: pack.names(), suggestions })
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn dismiss_suggestions(
    app: tauri::AppHandle,
    expected_root: String,
    pairs: Vec<classify::DismissPair>,
) -> Result<(), String> {
    let _operation = classify::operation_lock().lock().map_err(|_| "形象识别忙，请稍后再试")?;
    let state = app.state::<LibraryState>();
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    let library = current.as_ref().ok_or("请先打开资料库")?;
    library.check_expected_root(&expected_root)?;
    for pair in &pairs {
        library.asset_path(&pair.asset_id)?;
        if !(1..=50).contains(&pair.character.chars().count()) {
            return Err("形象名称须为 1–50 字".into());
        }
    }
    classify::append_dismissed(Path::new(&library.snapshot().root), pairs)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_drag::init())
        .setup(|app| {
            if let Ok(config) = app.path().app_config_dir() {
                let library = preferences::reopen(&config, |library| {
                    if authorize_assets(app.handle(), library).is_empty() { Ok(()) }
                    else { Err("资料库预览授权失败".into()) }
                });
                *app.state::<LibraryState>().0.lock().map_err(|_| "资料库忙")? = library;
            }
            Ok(())
        })
        .manage(LibraryState::default())
        .invoke_handler(tauri::generate_handler![
            select_library,
            current_library,
            import_images,
            get_management,
            save_metadata,
            import_provenance,
            add_collection,
            batch_rename,
            batch_labels,
            set_trash,
            reveal_in_finder,
            set_favorite,
            mark_used,
            prepare_wechat,
            inspect_library_folder,
            recent_libraries,
            ensure_thumbnails,
            copy_to_clipboard,
            asset_paths,
            drag_icon,
            delete_permanently,
            empty_trash,
            rename_collection,
            delete_collection,
            backup_library,
            restore_backup,
            scan_duplicates,
            create_group,
            disband_group,
            save_group_metadata,
            ignore_pair,
            export_assets,
            collect_douyin_fetch,
            collect_douyin_import,
            import_wechat_manifest,
            collect_wechat_import,
            wechat_detect_accounts,
            wechat_check_running,
            wechat_dump_and_export,
            wechat_grant_bookmark,
            classifier_status,
            enable_classifier,
            disable_classifier,
            classify_library,
            dismiss_suggestions
        ])
        .run(tauri::generate_context!())
        .expect("无法启动拾趣桌面应用");
}

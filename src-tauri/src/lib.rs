pub mod backup;
pub mod bookmark;
pub mod library;
pub mod management;
pub mod similar;
pub mod wechat;
pub mod wechat_db;
use management::ManagementSnapshot;

use backup::BackupSummary;
use library::{ImportReport, Library, LibrarySnapshot};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{Manager, State};

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
                if PathBuf::from(&path).canonicalize().ok().as_ref()
                    == Some(&PathBuf::from(library.snapshot().root))
                {
                    return Ok(library.snapshot());
                }
            }
        }
        let library = if create {
            Library::create(&PathBuf::from(path))?
        } else {
            Library::open(&PathBuf::from(path))?
        };
        let warnings = authorize_assets(&app, &library);
        if !warnings.is_empty() {
            return Err(format!("资料库预览授权失败：{}", warnings.join("；")));
        }
        let snapshot = library.snapshot();
        // A failed selection leaves the currently open library untouched.
        *current = Some(library);
        Ok(snapshot)
    })
    .await
    .map_err(|e| e.to_string())?
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
) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current
        .as_ref()
        .ok_or("请先打开资料库")?
        .add_collection_name(&expected_root, name)
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
async fn restore_backup(backup_dir: String, target_parent: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        backup::restore(&PathBuf::from(backup_dir), &PathBuf::from(target_parent))
            .map(|path| path.to_string_lossy().into_owned())
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
async fn collect_douyin_fetch(
    app: tauri::AppHandle,
    chrome_port: Option<u16>,
) -> Result<backup::CollectStageResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LibraryState>();
        let current = state.0.lock().map_err(|_| "资料库忙，请稍后再试")?;
        let scripts = scripts_dir();
        current
            .as_ref()
            .ok_or("请先打开资料库")?
            .collect_douyin_fetch(&scripts, chrome_port.unwrap_or(9222))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn collect_douyin_import(app: tauri::AppHandle) -> Result<CollectImportResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LibraryState>();
        let mut current = state.0.lock().map_err(|_| "资料库忙，请稍后再试")?;
        let library = current.as_mut().ok_or("请先打开资料库")?;
        let (report, report_path) = library.collect_douyin_import()?;
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

/// Scripts live next to the project, both in `tauri dev` (cwd = src-tauri)
/// and when started from the project root. Double-clicking the .app leaves
/// cwd elsewhere; probe from the executable's ancestors as well.
fn scripts_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut candidates = vec![cwd.join("scripts"), cwd.join("../scripts")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(anchor) = exe.parent() {
            for ancestor in anchor.ancestors() {
                candidates.push(ancestor.join("scripts"));
            }
        }
    }
    for candidate in candidates {
        if candidate.join("collect_douyin.mjs").is_file()
            || candidate.join("download_wechat.py").is_file()
        {
            return candidate;
        }
    }
    cwd.join("scripts")
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
        let scripts = scripts_dir();
        current
            .as_ref()
            .ok_or("请先打开资料库")?
            .import_wechat_manifest(&scripts, Path::new(&manifest_path))
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

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(LibraryState::default())
        .invoke_handler(tauri::generate_handler![
            select_library,
            current_library,
            import_images,
            get_management,
            save_metadata,
            import_provenance,
            batch_rename,
            batch_labels,
            set_trash,
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
            wechat_grant_bookmark
        ])
        .run(tauri::generate_context!())
        .expect("无法启动拾趣桌面应用");
}

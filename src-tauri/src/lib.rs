pub mod backup;
pub mod library;
pub mod management;
pub mod similar;
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
) -> Result<ManagementSnapshot, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    current.as_ref().ok_or("请先打开资料库")?.import_provenance(
        &expected_root,
        &PathBuf::from(path),
        account_alias,
        account_id,
    )
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
fn scan_duplicates(state: State<'_, LibraryState>) -> Result<similar::ScanReport, String> {
    let current = state.0.lock().map_err(|_| "资料库忙")?;
    let library = current.as_ref().ok_or("请先打开资料库")?;
    // Runs on the command thread: the file lock stays held by the kept state,
    // and the scan reads assets under that guarantee. ~1500 assets fits here.
    let management = library.get_management()?;
    let grouped: std::collections::HashSet<String> = management
        .groups
        .iter()
        .flat_map(|g| g.member_ids.clone())
        .collect();
    library.scan_static_duplicates(&management.ignored_pairs, &grouped)
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
            export_assets
        ])
        .run(tauri::generate_context!())
        .expect("无法启动拾趣桌面应用");
}

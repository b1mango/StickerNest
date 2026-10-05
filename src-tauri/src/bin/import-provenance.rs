use std::{ffi::OsString, path::PathBuf, process::ExitCode};
use stickernest_lib::library::Library;

const HELP: &str = "拾趣本地来源映射导入（不联网）
用法：import-provenance --library DIR --report FILE --alias 别名 [--account-id ID]
  --library     已有资料库根目录
  --report      本地采集报告（report.json）
  --alias       首次导入的账号别名；使用 --account-id 时忽略
  --account-id  已有账号标识（可选，用于重导）
  --help        显示帮助（单独使用）
同一命令是幂等的：重复导入同一报告不会增加账号、引用或批次。完全成功时输出导入前后计数。";

fn main() -> ExitCode {
    match run(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<OsString>) -> Result<(), String> {
    if args.len() == 1 && args[0] == "--help" {
        println!("{HELP}");
        return Ok(());
    }
    let mut library = None;
    let mut report = None;
    let mut alias = None;
    let mut account_id = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let (slot, label) = match arg.to_str() {
            Some("--library") => (&mut library, "--library"),
            Some("--report") => (&mut report, "--report"),
            Some("--alias") => (&mut alias, "--alias"),
            Some("--account-id") => (&mut account_id, "--account-id"),
            _ => return Err(format!("未知参数：{}", arg.to_string_lossy())),
        };
        let value = args
            .next()
            .ok_or_else(|| format!("参数缺少值：{label}"))?;
        if slot.replace(value).is_some() {
            return Err(format!("重复参数：{label}"));
        }
    }
    let library = library.ok_or("缺少 --library")?;
    let report = report.ok_or("缺少 --report")?;
    let alias = alias.ok_or("缺少 --alias")?;
    let alias = alias
        .into_string()
        .map_err(|_| "账号别名必须是有效文本")?;
    let account_id = account_id
        .map(|value| value.into_string().map_err(|_| "账号标识必须是有效文本"))
        .transpose()?;
    let library = Library::open(&PathBuf::from(library))?;
    let before = library.get_management()?;
    let before_counts = (
        before.accounts.len(),
        before.references.len(),
        before.batches.len(),
    );
    let after = library.import_provenance(
        &library.snapshot().root,
        &PathBuf::from(report),
        alias,
        account_id,
        None,
    )?;
    let account = after
        .accounts
        .iter()
        .map(|account| account.alias.clone())
        .collect::<Vec<_>>()
        .join("、");
    println!(
        "导入完成：账号 [{}]，账号/引用/批次 {}/{}/{} → {}/{}/{}",
        account,
        before_counts.0,
        before_counts.1,
        before_counts.2,
        after.accounts.len(),
        after.references.len(),
        after.batches.len()
    );
    if let Some(latest) = after.batches.last() {
        let unprocessed = latest
            .collection_items
            .saturating_sub(latest.mapped_resources + latest.failed_resources);
        println!(
            "最近批次：清单 {} 项 · 已映射 {} 项 · 失败 {} 项 · 未处理 {} 项",
            latest.collection_items, latest.mapped_resources, latest.failed_resources, unprocessed
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn import_is_idempotent_and_reports_counts() {
        let dir = tempfile::tempdir().unwrap();
        let mut library = Library::create(dir.path()).unwrap();
        let mut paths = vec![];
        for index in 0..2 {
            let path = dir.path().join(format!("{index}.png"));
            image::RgbaImage::from_pixel(2, 2, image::Rgba([index as u8, 0, 0, 255]))
                .save(&path)
                .unwrap();
            paths.push(path);
        }
        library.import_files(paths, "抖音".into()).unwrap();
        let id = library.snapshot().items[0].id.clone();
        let root = library.snapshot().root;
        let report_path = dir.path().join("report.json");
        fs::write(
            &report_path,
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 1, "expected_items": 2, "items": [
                    {"id": "100", "status": "verified", "sha256": id, "resource_identity": "a".repeat(64)},
                    {"id": "200", "status": "verified", "sha256": id, "resource_identity": "a".repeat(64)}
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        let make_args = || {
            vec![
                "--library".into(),
                root.clone().into(),
                "--report".into(),
                report_path.clone().into_os_string(),
                "--alias".into(),
                "我的抖音".into(),
            ]
        };
        drop(library);
        run(make_args()).unwrap();
        // Re-importing with the same alias must be rejected, like the GUI path.
        assert!(run(make_args()).is_err());
        let library = Library::open(&PathBuf::from(&root)).unwrap();
        let account_id = library.get_management().unwrap().accounts[0].id.clone();
        drop(library);
        let mut reimport = make_args();
        reimport.truncate(4);
        reimport.push("--alias".into());
        reimport.push("忽略".into());
        reimport.push("--account-id".into());
        reimport.push(account_id.into());
        run(reimport).unwrap();
        let library = Library::open(&PathBuf::from(&root)).unwrap();
        let management = library.get_management().unwrap();
        assert_eq!(management.accounts.len(), 1);
        assert_eq!(management.accounts[0].alias, "我的抖音");
        assert_eq!(management.references.len(), 2);
        assert_eq!(management.batches.len(), 1);
    }
    #[test]
    fn missing_arguments_are_rejected() {
        assert!(run(vec![]).is_err());
        assert!(run(vec!["--library".into()]).is_err());
        assert!(run(vec!["--unknown".into()]).is_err());
    }
}

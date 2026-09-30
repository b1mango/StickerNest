use serde::Serialize;
use std::{ffi::OsString, fs, path::PathBuf, process::ExitCode};
use stickernest_lib::library::{ImportReport, Library};

const HELP: &str = "拾趣本地批量导入（不联网，不递归）
用法：import-local --input DIR --library DIR --source 本地|微信|抖音 [--create]
  --input    仅包含图片资源的目录；允许 png/jpg/jpeg/gif/webp/awebp/bin
  --library  默认是已有资料库根目录；加 --create 时是已存在的父目录，
             将在其中新建 StickerNest Library，绝不覆盖已有资料库
  --source   必填素材来源
  --create   新建资料库
  --help     显示帮助（单独使用）
输出：成功执行时 stdout 为 JSON；含单文件失败时退出码为 2，其他错误为 1。
导入非事务性：单文件失败不回滚此前成功素材；可重新运行去重。";

#[derive(Debug)]
struct Options {
    input: PathBuf,
    library: PathBuf,
    source: String,
    create: bool,
}

fn parse(args: Vec<OsString>) -> Result<Option<Options>, String> {
    if args.len() == 1 && args[0] == "--help" {
        return Ok(None);
    }
    let mut input = None;
    let mut library = None;
    let mut source = None;
    let mut create = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--create" {
            if create {
                return Err("重复参数：--create".into());
            }
            create = true;
            continue;
        }
        if arg != "--input" && arg != "--library" && arg != "--source" {
            return Err(format!("未知参数：{}", arg.to_string_lossy()));
        }
        let value = args
            .next()
            .ok_or_else(|| format!("参数缺少值：{}", arg.to_string_lossy()))?;
        if value.is_empty() || value.to_string_lossy().starts_with("--") {
            return Err(format!("参数缺少值：{}", arg.to_string_lossy()));
        }
        let duplicate = if arg == "--input" {
            input.replace(PathBuf::from(value)).is_some()
        } else if arg == "--library" {
            library.replace(PathBuf::from(value)).is_some()
        } else {
            let value = value
                .into_string()
                .map_err(|_| "来源必须为本地、微信或抖音")?;
            if !matches!(value.as_str(), "本地" | "微信" | "抖音") {
                return Err("来源必须为本地、微信或抖音".into());
            }
            source.replace(value).is_some()
        };
        if duplicate {
            return Err(format!("重复参数：{}", arg.to_string_lossy()));
        }
    }
    Ok(Some(Options {
        input: input.ok_or("缺少 --input")?,
        library: library.ok_or("缺少 --library")?,
        source: source.ok_or("缺少 --source")?,
        create,
    }))
}

fn inputs(directory: &std::path::Path) -> Result<Vec<PathBuf>, String> {
    if !fs::symlink_metadata(directory)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_dir()
    {
        return Err("输入必须是实际目录，不能是符号链接".into());
    }
    let mut paths = fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    paths.sort();
    for path in &paths {
        if !fs::symlink_metadata(path)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_file()
        {
            return Err(format!("输入包含非普通文件或符号链接：{}", path.display()));
        }
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(
            extension.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "awebp" | "bin"
        ) {
            return Err(format!(
                "输入目录包含非素材文件，请移出后重试：{}",
                path.display()
            ));
        }
    }
    if paths.is_empty() {
        return Err("输入目录没有素材".into());
    }
    Ok(paths)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    library: String,
    input_count: usize,
    total_items: usize,
    report: ImportReport,
}

fn run(options: Options) -> Result<Output, String> {
    // Validate the complete input before creating or mutating a library.
    let paths = inputs(&options.input)?;
    let input_count = paths.len();
    let mut library = if options.create {
        Library::create(&options.library)?
    } else {
        Library::open(&options.library)?
    };
    let report = library.import_files(paths, options.source)?;
    let snapshot = library.snapshot();
    Ok(Output {
        library: snapshot.root,
        input_count,
        total_items: snapshot.items.len(),
        report,
    })
}

fn main() -> ExitCode {
    let result = parse(std::env::args_os().skip(1).collect()).and_then(|options| {
        let Some(options) = options else {
            println!("{HELP}");
            return Ok(ExitCode::SUCCESS);
        };
        let output = run(options)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?
        );
        Ok(if output.report.failed.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(2)
        })
    });
    result.unwrap_or_else(|error| {
        eprintln!("{error}");
        ExitCode::FAILURE
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(extra: &[&str]) -> Vec<OsString> {
        [
            "--input",
            "resources",
            "--library",
            "parent",
            "--source",
            "抖音",
        ]
        .into_iter()
        .chain(extra.iter().copied())
        .map(OsString::from)
        .collect()
    }

    #[test]
    fn parameters_are_explicit_and_unambiguous() {
        let options = parse(args(&["--create"])).unwrap().unwrap();
        assert!(options.create);
        assert_eq!(options.source, "抖音");
        for extra in [
            vec!["--recursive"],
            vec!["--input", "other"],
            vec!["--library", "other"],
            vec!["--source", "微信"],
            vec!["--create", "--create"],
            vec!["--input"],
            vec!["--help"],
        ] {
            assert!(parse(args(&extra)).is_err(), "{extra:?}");
        }
        assert!(parse(vec![]).is_err());
        assert!(parse(vec!["--help".into()]).unwrap().is_none());
        assert!(parse(vec!["--input".into(), "--library".into()]).is_err());
    }

    #[test]
    fn directory_is_sorted_and_rejects_non_resources() {
        let dir = tempfile::tempdir().unwrap();
        assert!(inputs(dir.path()).is_err());
        fs::write(dir.path().join("b.bin"), b"").unwrap();
        fs::write(dir.path().join("a.WEBP"), b"").unwrap();
        let paths = inputs(dir.path()).unwrap();
        assert_eq!(paths[0].file_name().unwrap(), "a.WEBP");
        fs::write(dir.path().join("manifest.json"), b"{}").unwrap();
        assert!(inputs(dir.path()).is_err());
        fs::remove_file(dir.path().join("manifest.json")).unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        assert!(inputs(dir.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_before_creating_library() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let resources = dir.path().join("resources");
        fs::create_dir(&resources).unwrap();
        fs::write(dir.path().join("source.png"), b"").unwrap();
        symlink(dir.path().join("source.png"), resources.join("image.png")).unwrap();
        assert!(run(Options {
            input: resources.clone(),
            library: dir.path().into(),
            source: "抖音".into(),
            create: true
        })
        .is_err());
        assert!(!dir.path().join("StickerNest Library").exists());
        let alias = dir.path().join("alias");
        symlink(resources, &alias).unwrap();
        assert!(inputs(&alias).is_err());
    }

    #[test]
    fn imports_original_bytes_and_reports_duplicates_and_failures() {
        let dir = tempfile::tempdir().unwrap();
        let resources = dir.path().join("resources");
        fs::create_dir(&resources).unwrap();
        let original = resources.join("a.png");
        image::RgbaImage::new(2, 2).save(&original).unwrap();
        fs::copy(&original, resources.join("b.bin")).unwrap();
        fs::write(resources.join("broken.webp"), b"invalid").unwrap();
        let output = run(Options {
            input: resources,
            library: dir.path().into(),
            source: "抖音".into(),
            create: true,
        })
        .unwrap();
        assert_eq!(
            (
                output.input_count,
                output.total_items,
                output.report.added,
                output.report.duplicates,
                output.report.failed.len()
            ),
            (3, 1, 1, 1, 1)
        );
        let library = Library::open(&PathBuf::from(output.library)).unwrap();
        let sticker = &library.snapshot().items[0];
        assert_eq!(sticker.sources, ["抖音"]);
        assert_eq!(
            fs::read(library.asset_path(&sticker.id).unwrap()).unwrap(),
            fs::read(original).unwrap()
        );
    }
}

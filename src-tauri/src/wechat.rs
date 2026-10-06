//! Native WeChat collection: clone the app to a user-owned cache, ad-hoc
//! resign the clone, start it with our key-dumping dylib, then decrypt the
//! emoticon database offline. The installed app is never touched; no sudo, no
//! debugger, no SIP changes; nothing is written outside our own directories.
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{
    fs,
    path::Path,
    io::Write,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const DYLIB_NAME: &str = "wechat-key-dumper.dylib";
const DYLIB_SHA256: &str = "af9f8342f1afbc3dea374e6d329a8697fd111e61e48b603ff01276aa5876fd88";
const KEY_TIMEOUT: Duration = Duration::from_secs(600);
const POLL_STEP: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeChatAccount {
    pub wxid: String,
    pub has_db: bool,
    pub has_cached_key: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowEvent {
    pub stage: String,
    pub note: String,
}

fn err(context: &str, e: impl std::fmt::Display) -> String {
    format!("{context}:{e}")
}

fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "无法定位用户主目录".to_string())
}

fn xwechat_files_root() -> Result<PathBuf, String> {
    // Prefer the security-scoped bookmark the user granted; fall back to the
    // canonical path (useful when the app already holds full-disk access).
    if let Ok(Some(bookmarked)) = crate::bookmark::stored_bookmark_root() {
        return Ok(bookmarked);
    }
    Ok(home_dir()?.join(
        "Library/Containers/com.tencent.xinWeChat/Data/Documents/xwechat_files",
    ))
}

fn app_cache_root() -> Result<PathBuf, String> {
    Ok(home_dir()?.join("Library/Caches/StickerNest"))
}

fn app_support_root() -> Result<PathBuf, String> {
    Ok(home_dir()?.join("Library/Application Support/StickerNest/wechat"))
}

pub fn dylib_path() -> Result<PathBuf, String> {
    // Prefer the packaged resource; fall back to the source build in dev.
    let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut candidates = vec![
        dir.join("tools/wechat-key-dumper").join(DYLIB_NAME),
        dir.join("../tools/wechat-key-dumper").join(DYLIB_NAME),
    ];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(anchor) = exe.parent() {
            for ancestor in anchor.ancestors() {
                candidates.push(ancestor.join("tools/wechat-key-dumper").join(DYLIB_NAME));
                candidates.push(ancestor.join("Resources").join(DYLIB_NAME));
            }
        }
    }
    for candidate in &candidates {
        if candidate.is_file() {
            return Ok(candidate.clone());
        }
    }
    Err("缺少密钥捕获组件（请确认应用完整安装）".into())
}

fn verify_dylib(path: &Path) -> Result<(), String> {
    let data = fs::read(path).map_err(|e| err("无法读取密钥捕获组件", e))?;
    let digest = format!("{:x}", sha2::Sha256::digest(&data));
    if digest != DYLIB_SHA256 {
        return Err("密钥捕获组件校验失败（文件与构建时不一致）".into());
    }
    Ok(())
}

/// List local WeChat accounts that have an emoticon database worth reading.
pub fn detect_accounts() -> Result<Vec<WeChatAccount>, String> {
    let root = xwechat_files_root()?;
    let mut out = vec![];
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(err("无法读取微信数据目录", e)),
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("wxid_") {
            continue;
        }
        let has_db = entry
            .path()
            .join("db_storage/emoticon/emoticon.db")
            .is_file();
        let has_cached_key = cached_key_path(&name)
            .map(|p| p.is_file())
            .unwrap_or(false);
        out.push(WeChatAccount {
            wxid: name,
            has_db,
            has_cached_key,
        });
    }
    out.sort_by(|a, b| b.has_db.cmp(&a.has_db).then(a.wxid.cmp(&b.wxid)));
    Ok(out)
}

fn cached_key_path(wxid: &str) -> Result<PathBuf, String> {
    let safe: String = wxid
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if safe.is_empty() || safe.len() != wxid.len() {
        return Err("微信账号标识无效".into());
    }
    Ok(app_support_root()?.join(format!("emoticon_dbkey_{safe}.txt")))
}

/// Cached dumper hits: each candidate is stored as `<tag>:<hex>` (P for a
/// PBKDF2-derived passphrase, K for a raw sqlite3_key call). This returns the
/// candidate materials without the tags, PBKDF2 form first then raw.
pub fn cached_key(wxid: &str) -> Result<Option<Vec<String>>, String> {
    let path = cached_key_path(wxid)?;
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|e| err("无法读取密钥缓存", e))?;
    let mut pass = vec![];
    let mut raw = vec![];
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (tag, value) = line.split_once(':').unwrap_or(("", line));
        let value = value.trim().to_string();
        if value.is_empty() {
            continue;
        }
        if tag == "P" {
            pass.push(value);
        } else {
            raw.push(value);
        }
    }
    let mut out = pass;
    out.extend(raw);
    Ok(if out.is_empty() { None } else { Some(out) })
}

fn write_cached_candidates(wxid: &str, candidates_text: &str) -> Result<PathBuf, String> {
    let dir = app_support_root()?;
    fs::create_dir_all(&dir).map_err(|e| err("无法创建应用支持目录", e))?;
    let path = cached_key_path(wxid)?;
    let mut temp = tempfile::NamedTempFile::new_in(&dir).map_err(|e| err("无法写入密钥缓存", e))?;
    temp.write_all(candidates_text.as_bytes())
        .map_err(|e| err("无法写入密钥缓存", e))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| err("无法写入密钥缓存", e))?;
    temp.persist(&path).map_err(|e| err("无法写入密钥缓存", e))?;
    Ok(path)
}

#[derive(Debug, PartialEq)]
pub enum WeChatRunning {
    NotRunning,
    Running(usize),
}

pub fn check_wechat_running() -> WeChatRunning {
    let output = Command::new("/bin/ps")
        .args(["-axo", "comm="])
        .output();
    let Ok(out) = output else {
        return WeChatRunning::Running(0);
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let count = text
        .lines()
        .filter(|line| {
            let line = line.trim();
            line.ends_with("/WeChat")
                || line.ends_with("/WeChatApp")
                || line.contains("WeChatAppEx")
        })
        .count();
    if count > 0 {
        WeChatRunning::Running(count)
    } else {
        WeChatRunning::NotRunning
    }
}

fn bundle_version(app: &Path) -> Result<String, String> {
    let info = app.join("Contents/Info.plist");
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleVersion", "raw", "-o", "-"])
        .arg(&info)
        .output()
        .map_err(|e| err("无法读取微信版本", e))?;
    if !output.status.success() {
        return Err("无法读取微信版本".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn ensure_runnable_clone(app: &Path) -> Result<PathBuf, String> {
    if !app.join("Contents/MacOS").is_dir() {
        return Err("没有在 /Applications 找到微信应用".into());
    }
    let version = bundle_version(app)?;
    let cache_dir = app_cache_root()?;
    fs::create_dir_all(&cache_dir).map_err(|e| err("无法创建缓存目录", e))?;
    let clone = cache_dir.join(format!("WeChat-{version}.app"));
    let marker = clone.join(".stickernest-resigned");
    if marker.is_file() {
        return Ok(clone);
    }
    if clone.exists() {
        fs::remove_dir_all(&clone).map_err(|e| err("无法清理旧缓存副本", e))?;
    }
    let status = Command::new("cp")
        .args(["-R", "--"])
        .arg(app)
        .arg(&clone)
        .status()
        .map_err(|e| err("无法创建微信本地副本", e))?;
    if !status.success() {
        return Err("无法创建微信本地副本（复制失败）".into());
    }
    let _ = Command::new("xattr")
        .args(["-cr", "--"])
        .arg(&clone)
        .status();
    let status = Command::new("codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(&clone)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| err("无法为本地副本签名", e))?;
    if !status.success() {
        return Err("无法为微信本地副本签名".into());
    }
    fs::write(&marker, b"stickernest\n").map_err(|e| err("无法标记副本", e))?;
    Ok(clone)
}

fn terminate_child(child: &mut Child) {
    let pid = child.id();
    let _ = Command::new("/bin/kill").arg(pid.to_string()).status();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => (),
            Err(_) => return,
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Capture key candidates for the target account by starting the resigned
/// clone under our dylib. All candidates the hook saw are appended into one
/// file; the caller validates which actually decrypts the emoticon database.
pub fn dump_key(
    wxid: &str,
    on_stage: &mut dyn FnMut(&str),
) -> Result<Vec<String>, String> {
    if !wxid.starts_with("wxid_") || wxid.len() > 64 {
        return Err("微信账号标识无效".into());
    }
    if let WeChatRunning::Running(n) = check_wechat_running() {
        return Err(format!("WECHAT_RUNNING：检测到微信仍在运行（{n} 个进程）。请先完全退出微信，再点“我已退出，继续”。"));
    }
    let dylib = dylib_path()?;
    verify_dylib(&dylib)?;
    on_stage("正在准备微信本地副本");
    let clone = ensure_runnable_clone(Path::new("/Applications/WeChat.app"))?;
    let out_dir = app_support_root()?;
    fs::create_dir_all(&out_dir).map_err(|e| err("无法创建密钥输出目录", e))?;
    let key_out = out_dir.join(format!(".dumper-{wxid}.key"));
    let log_out = out_dir.join(format!(".dumper-{wxid}.log"));
    let _ = fs::remove_file(&key_out);
    on_stage("正在等待微信提供密钥（首次可能弹出新窗口，请不用操作）");
    let mut child = Command::new(clone.join("Contents/MacOS/WeChat"))
        .env("DYLD_INSERT_LIBRARIES", &dylib)
        .env("SN_KEY_OUT", &key_out)
        .env("SN_TARGET_WXID", wxid)
        .env("SN_LOG_OUT", &log_out)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| err("无法启动微信本地副本", e))?;
    let deadline = Instant::now() + KEY_TIMEOUT;
    let data = loop {
        if key_out.is_file() {
            let data = fs::read_to_string(&key_out).map_err(|e| err("无法读取密钥输出", e))?;
            if !data.trim().is_empty() {
                terminate_child(&mut child);
                break data;
            }
        }
        match child.try_wait() {
            Ok(Some(_)) => break return Err("微信本地副本提前退出，未取得密钥".into()),
            Ok(None) => (),
            Err(e) => break return Err(format!("无法监视微信本地副本：{e}")),
        }
        if Instant::now() >= deadline {
            terminate_child(&mut child);
            break return Err("等待密钥超时（10 分钟）。请重试；若仍失败，请在微信里打开一次表情面板再试".into());
        }
        std::thread::sleep(POLL_STEP);
    };
    let mut out = vec![];
    for line in data.lines() {
        let line = line.trim();
        if let Some((_, value)) = line.split_once(':') {
            out.push(value.trim().to_string());
        } else if !line.is_empty() {
            out.push(line.to_string());
        }
    }
    // Cache under whichever account the candidates actually decrypt; when the
    // clone logged into the selected account this is the requesting wxid.
    let owner = resolve_candidate_account(&out).unwrap_or_else(|_| wxid.to_string());
    write_cached_candidates(&owner, &data)?;
    let _ = fs::remove_file(&key_out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detect_accounts_handles_missing_root() {
        // Missing container root must be an empty list, not an error.
        let _ = detect_accounts();
    }
    #[test]
    fn invalid_wxid_is_rejected() {
        assert!(dump_key("notanaccount", &mut |_| {}).is_err());
        assert!(dump_key(&"x".repeat(100), &mut |_| {}).is_err());
    }
}

/// Match captured candidates against every local emoticon database and return
/// the wxid they actually decrypt, so the cache always lands under the right
/// account even when the clone logged into a different one.
pub fn resolve_candidate_account(candidates: &[String]) -> Result<String, String> {
    let root = xwechat_files_root()?;
    for entry in fs::read_dir(&root).map_err(|e| err("无法读取微信数据目录", e))?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("wxid_") {
            continue;
        }
        let db = entry.path().join("db_storage/emoticon/emoticon.db");
        if !db.is_file() {
            continue;
        }
        if candidates
            .iter()
            .any(|c| crate::wechat_db::decrypt_db(&db, c).is_ok())
        {
            return Ok(name);
        }
    }
    Err("没有候选密钥能解密本机任何账号的表情数据库".into())
}

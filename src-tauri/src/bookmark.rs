//! Security-scoped bookmark for the WeChat container directory: the user
//! confirms a one-shot folder picker, we store the bookmark base64 in our own
//! support dir, and from then on the app can read the WeChat data without any
//! system-level grant.
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{
    ffi::{c_char, c_void, CString},
    fs,
    path::PathBuf,
};

type SnFn = unsafe extern "C" fn(*const c_char, *mut c_char, i32) -> i32;

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

const RTLD_NOW: i32 = 2;

const HELPER_NAME: &str = "bookmark-helper.dylib";
const HELPER_SHA256: &str = "95caff79e592b27216004214809ac312dfea4ffe7397f4a7757c56787eda5278";
const BUF_LEN: usize = 16 * 1024;

struct Helper(*mut c_void);

impl Helper {
    fn load() -> Result<Self, String> {
        let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut candidates = vec![
            dir.join("tools/bookmark-helper").join(HELPER_NAME),
            dir.join("../tools/bookmark-helper").join(HELPER_NAME),
        ];
        if let Ok(exe) = std::env::current_exe() {
            if let Some(anchor) = exe.parent() {
                for ancestor in anchor.ancestors() {
                    candidates.push(ancestor.join("tools/bookmark-helper").join(HELPER_NAME));
                    candidates.push(ancestor.join("Resources").join(HELPER_NAME));
                }
            }
        }
        for candidate in candidates {
            if candidate.is_file() {
                let data = fs::read(&candidate).map_err(|e| e.to_string())?;
                let digest = format!("{:x}", sha2::Sha256::digest(&data));
                if digest != HELPER_SHA256 {
                    return Err("书签辅助组件校验失败（文件与构建时不一致）".into());
                }
                let path = CString::new(candidate.to_string_lossy().into_owned())
                    .map_err(|e| e.to_string())?;
                let handle = unsafe { dlopen(path.as_ptr(), RTLD_NOW) };
                if handle.is_null() {
                    return Err("书签辅助组件无法加载".into());
                }
                return Ok(Helper(handle));
            }
        }
        Err("缺少书签辅助组件（请确认应用完整安装）".into())
    }
    fn call(&self, symbol: &str, input: &str) -> Result<String, String> {
        let sym_name = CString::new(symbol).map_err(|e| e.to_string())?;
        let ptr = unsafe { dlsym(self.0, sym_name.as_ptr()) };
        if ptr.is_null() {
            return Err("书签辅助组件入口缺失".into());
        }
        let func: SnFn = unsafe { std::mem::transmute(ptr) };
        let mut out = vec![0i8; BUF_LEN];
        let input_c = CString::new(input).map_err(|e| e.to_string())?;
        let n = unsafe { func(input_c.as_ptr(), out.as_mut_ptr(), BUF_LEN as i32) };
        if n <= 0 {
            return Err("书签辅助组件调用失败".into());
        }
        let bytes: Vec<u8> = out[..n as usize].iter().map(|b| *b as u8).collect();
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
    fn store(&self, path: &str) -> Result<String, String> {
        self.call("sn_bookmark_store", path)
    }
    fn resolve(&self, base64: &str) -> Result<String, String> {
        self.call("sn_bookmark_resolve", base64)
    }
}

fn bookmark_file() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "无法定位用户主目录".to_string())?;
    Ok(home.join("Library/Application Support/StickerNest/wechat/container.bookmark"))
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BookmarkRecord {
    version: u32,
    base64: String,
}

pub fn stored_bookmark_root() -> Result<Option<PathBuf>, String> {
    let path = bookmark_file()?;
    if !path.is_file() {
        return Ok(None);
    }
    let data = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let record: BookmarkRecord = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    if record.version != 1 {
        return Err("书签记录版本不符".into());
    }
    let helper = Helper::load()?;
    let resolved = helper.resolve(&record.base64)?;
    Ok(Some(PathBuf::from(resolved)))
}

pub fn store_bookmark(path: &str) -> Result<(), String> {
    let helper = Helper::load()?;
    let base64 = helper.store(path)?;
    let record = BookmarkRecord {
        version: 1,
        base64,
    };
    let target = bookmark_file()?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut temp = tempfile::NamedTempFile::new_in(target.parent().ok_or("书签目录无效")?)
        .map_err(|e| e.to_string())?;
    use std::io::Write;
    temp.write_all(serde_json::to_string(&record).map_err(|e| e.to_string())?.as_bytes())
        .map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(&target).map_err(|e| e.to_string())?;
    Ok(())
}

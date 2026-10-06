//! Offline SQLCipher v4 decryption of the WeChat emoticon database, plus the
//! favourite-sticker URL listing. Semantics follow the public reference
//! implementations (liusheng22/export-wechat-emoji decrypt_db_file_v4; MIT).
use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha512};
use std::{collections::HashSet, fs, path::PathBuf};

const PAGE_SIZE: usize = 4096;
const IV_SIZE: usize = 16;
const RESERVE: usize = 80;
const PBKDF2_ITER: u32 = 256_000;
const MAC_ROUNDS: u32 = 2;
const KEY_LEN: usize = 32;
const MAC_LEN: usize = 64;
const SALT_LEN: usize = 16;
const SQLITE_HEADER: &[u8] = b"SQLite format 3";

type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
type HmacSha512 = Hmac<Sha512>;

pub struct DecryptedDb {
    pub pages: Vec<Vec<u8>>,
    pub page_count: usize,
}

fn pbkdf32(password: &[u8], salt: &[u8], rounds: u32) -> [u8; KEY_LEN] {
    let mut out = [0u8; KEY_LEN];
    pbkdf2::pbkdf2_hmac::<Sha512>(password, salt, rounds, &mut out);
    out
}

/// Parse the output of `sqlite3_key`'s raw key (64 hex chars) or a passphrase.
/// Returns None when neither form matches.
pub fn parse_key_material(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return (0..64)
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
            .collect();
    }
    if !text.is_empty() && text.is_ascii() && !text.bytes().any(|b| b < 0x20) {
        return Some(text.as_bytes().to_vec());
    }
    None
}

pub fn decrypt_db(path: &PathBuf, key_material: &str) -> Result<DecryptedDb, String> {
    let raw = fs::read(path).map_err(|e| format!("无法读取表情数据库:{e}"))?;
    if raw.starts_with(SQLITE_HEADER) {
        let pages = raw.chunks(PAGE_SIZE).map(|p| p.to_vec()).collect::<Vec<_>>();
        return Ok(DecryptedDb { page_count: pages.len(), pages });
    }
    if raw.len() < PAGE_SIZE || raw.len() % PAGE_SIZE != 0 {
        return Err("表情数据库结构异常（页大小不符）".into());
    }
    let salt: Vec<u8> = raw[..SALT_LEN].to_vec();
    let mac_salt: Vec<u8> = salt.iter().map(|b| b ^ 0x3a).collect();

    let mut candidates: Vec<[u8; KEY_LEN]> = vec![];
    if let Some(pass) = parse_key_material(key_material) {
        // pass-as-passphrase and pass-as-raw-key are both plausible; try both.
        candidates.push(pbkdf32(&pass, &salt, PBKDF2_ITER));
        if pass.len() == KEY_LEN {
            let mut raw_key = [0u8; KEY_LEN];
            raw_key.copy_from_slice(&pass);
            candidates.push(raw_key);
        }
    }
    if candidates.is_empty() {
        return Err("密钥格式无法识别".into());
    }
    let total_pages = raw.len() / PAGE_SIZE;
    for key in &candidates {
        let mac_key = pbkdf32(key, &mac_salt, MAC_ROUNDS);
        let mut pages: Vec<Vec<u8>> = Vec::with_capacity(total_pages);
        let mut ok = true;
        for cur in 0..total_pages {
            let offset = if cur == 0 { SALT_LEN } else { 0 };
            let start = cur * PAGE_SIZE;
            let end = start + PAGE_SIZE;
            let iv_start = end - RESERVE;
            let iv_end = iv_start + IV_SIZE;
            let hmac_start = iv_end;
            let hmac_end = hmac_start + MAC_LEN;
            let content = &raw[start + offset..iv_start];
            let iv = &raw[iv_start..iv_end];
            let mut mac = match <HmacSha512 as Mac>::new_from_slice(&mac_key) {
                Ok(mac) => mac,
                Err(e) => return Err(format!("hmac 初始化失败:{e}")),
            };
            mac.update(&raw[start + offset..iv_end]);
            mac.update(&((cur as u32) + 1).to_le_bytes());
            let expected = mac.finalize().into_bytes();
            if expected.as_slice() != &raw[hmac_start..hmac_end] {
                ok = false;
                break;
            }
            let mut buf = content.to_vec();
            let plain = match Aes256CbcDec::new(key.into(), iv.into())
                .decrypt_padded_mut::<NoPadding>(&mut buf)
            {
                Ok(plain) => plain.to_vec(),
                Err(_) => {
                    ok = false;
                    break;
                }
            };
            let mut page = Vec::with_capacity(PAGE_SIZE);
            if cur == 0 {
                page.extend_from_slice(SQLITE_HEADER);
                page.push(0x00);
            }
            page.extend_from_slice(&plain);
            // Reserve bytes stay as-is so sqlite's page layout is preserved.
            page.extend_from_slice(&raw[iv_start..end]);
            pages.push(page);
        }
        if ok {
            let page_count = pages.len();
            return Ok(DecryptedDb { pages, page_count });
        }
    }
    Err("表情数据库解密密钥无效（首次可能需要重新退出微信后重试获取）".into())
}

/// Collect favourite emoticon URLs by reading the decrypted pages as SQLite.
/// Each candidate is tried first as a raw cipher key, then as a passphrase
/// derived via PBKDF2 — the first one whose per-page HMAC verifies wins.
pub fn export_urls(
    db_path: &PathBuf,
    key_candidates: &[String],
    out_path: &PathBuf,
) -> Result<usize, String> {
    let mut last_err = String::new();
    let mut decrypted = None;
    for candidate in key_candidates {
        match decrypt_db(db_path, candidate) {
            Ok(d) => {
                decrypted = Some(d);
                break;
            }
            Err(e) => last_err = e,
        }
    }
    let decrypted = decrypted.ok_or(if last_err.is_empty() {
        "没有可尝试的密钥候选".to_string()
    } else {
        last_err
    })?;
    let temp_dir = out_path
        .parent()
        .ok_or_else(|| "清单输出路径无效".to_string())?;
    fs::create_dir_all(temp_dir).map_err(|e| e.to_string())?;
    let plain_path = temp_dir.join(".stickernest-decrypted.tmp");
    {
        use std::io::Write;
        let mut file = fs::File::create(&plain_path).map_err(|e| e.to_string())?;
        for page in &decrypted.pages {
            // Pages are already full-width (decrypt_db keeps the 80-byte
            // reserve tail), so write them sequentially into one plain db.
            file.write_all(page).map_err(|e| e.to_string())?;
        }
        file.sync_all().map_err(|e| e.to_string())?;
    }
    let urls = query_fav_urls(&plain_path)?;
    let _ = fs::remove_file(&plain_path);
    use std::io::Write;
    let mut file = fs::File::create(out_path).map_err(|e| e.to_string())?;
    for url in &urls {
        writeln!(file, "{url}").map_err(|e| e.to_string())?;
    }
    Ok(urls.len())
}

fn query_fav_urls(plain_path: &PathBuf) -> Result<Vec<String>, String> {
    use rusqlite::Connection;
    let conn = Connection::open_with_flags(
        plain_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("解密后的表情数据库无法打开:{e}"))?;

    let tables: HashSet<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")
        .map_err(|e| e.to_string())?
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let non_store_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(kNonStoreEmoticonTable)")
        .map_err(|e| e.to_string())?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    if non_store_cols.is_empty() {
        return Err("表情数据库结构与当前版本不一致（缺少收藏表）".into());
    }
    let url_candidates = ["cdn_url", "tp_url", "thumb_url", "extern_url", "encrypt_url"]
        .iter()
        .filter(|c| non_store_cols.iter().any(|x| x.eq_ignore_ascii_case(c)))
        .cloned()
        .collect::<Vec<_>>();
    if url_candidates.len() < 2 {
        return Err("表情数据库结构与当前版本不一致（缺少素材地址列）".into());
    }
    let best_url_expr = format!(
        "COALESCE(NULLIF({0}, ''), NULLIF({1}, ''), NULLIF({2}, ''), NULLIF({3}, ''), NULLIF({4}, ''))",
        url_candidates[0],
        url_candidates.get(1).copied().unwrap_or(url_candidates[0]),
        url_candidates.get(2).copied().unwrap_or(url_candidates[0]),
        url_candidates.get(3).copied().unwrap_or(url_candidates[0]),
        url_candidates.get(4).copied().unwrap_or(url_candidates[0])
    );

    let mut urls = vec![];
    let order_tables = ["kFavEmoticonOrderTable", "kCustomEmoticonOrderTable"];
    for order in order_tables {
        if !tables.contains(order) {
            continue;
        }
        let sql = format!(
            "SELECT s.md5, {best} FROM {order} o LEFT JOIN kNonStoreEmoticonTable s ON s.md5 = o.md5 ORDER BY o.rowid",
            order = order,
            best = best_url_expr
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(stmt) => stmt,
            Err(_) => continue,
        };
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?))
        });
        if let Ok(rows) = rows {
            for row in rows.flatten() {
                if let (Some(_md5), Some(url)) = row {
                    if !url.is_empty() && !urls.contains(&url) && urls.len() < 20_000 {
                        urls.push(url);
                    }
                }
            }
        }
        if !urls.is_empty() {
            return Ok(urls);
        }
    }
    Err("表情数据库里没有可导出的收藏地址".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_key_material_hex_and_passphrase() {
        let hex = "ab".repeat(32);
        let parsed = parse_key_material(&hex).unwrap();
        assert_eq!(parsed.len(), 32);
        let pass = parse_key_material("pass-phrase-123").unwrap();
        assert_eq!(pass, b"pass-phrase-123");
        assert!(parse_key_material("").is_none());
        assert!(parse_key_material("not-hex!").is_some());
        assert!(parse_key_material("zh\u{4e2d}").is_none());
    }
}

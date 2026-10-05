//! Offline SQLCipher v4 decryption of the WeChat emoticon database, plus the
//! favourite-sticker URL listing. Semantics follow the public reference
//! implementations (liusheng22/export-wechat-emoji decrypt_db_file_v4; MIT).
use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha512};
use std::{collections::HashSet, fs, path::PathBuf};

const PAGE_SIZE: usize = 4096;
const RESERVE: usize = 80;
const PBKDF2_ITER: u32 = 256_000;
const KEY_LEN: usize = 32;

type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

pub struct DecryptedDb {
    pub pages: Vec<Vec<u8>>,
    pub page_count: usize,
}

fn pbkdf(password: &[u8], salt: &[u8]) -> [u8; KEY_LEN * 2] {
    let mut out = [0u8; KEY_LEN * 2];
    pbkdf2::pbkdf2_hmac::<Sha512>(password, salt, PBKDF2_ITER, &mut out);
    out
}

/// Decrypt one SQLCipher 4.x page. Layout:
/// [ payload (PAGE_SIZE - RESERVE) | iv (16) | tag (32) | reserve pad (RESERVE-48) ]
fn decrypt_page(key: &[u8], page: &[u8], page_no: u64) -> Option<Vec<u8>> {
    if page.len() != PAGE_SIZE {
        return None;
    }
    let (payload, rest) = page.split_at(PAGE_SIZE - RESERVE);
    let (iv_bytes, tag_bytes) = rest.split_at(16);
    let tag_expected = &tag_bytes[..32];

    let mut mac = <Hmac<Sha512> as Mac>::new_from_slice(key).ok()?;
    mac.update(payload);
    mac.update(iv_bytes);
    mac.update(&page_no.to_le_bytes());
    let tag: [u8; 64] = mac.finalize().into_bytes().into();
    if tag[..32] != tag_expected[..] {
        return None;
    }
    let mut buf = payload.to_vec();
    Aes256CbcDec::new(key[..32].into(), iv_bytes.into())
        .decrypt_padded_mut::<NoPadding>(&mut buf)
        .ok()?;
    Some(buf)
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
    if raw.len() < PAGE_SIZE || raw.len() % PAGE_SIZE != 0 {
        return Err("表情数据库结构异常（页大小不符）".into());
    }
    let salt = &raw[..16];
    let candidates: Vec<Vec<u8>> = {
        let mut v = vec![];
        if let Some(pb) = parse_key_material(key_material) {
            v.push(pbkdf(&pb, salt).to_vec());
            // SQLCipher mac variant used by WeChat 4.x on Apple platforms.
            let mac_salt: Vec<u8> = salt.iter().map(|b| b ^ 0x3a).collect();
            v.push(pbkdf(&pb, &mac_salt).to_vec());
        }
        if let Some(raw_key) = parse_key_material(key_material).filter(|k| k.len() == 32) {
            v.push(raw_key);
            let mac_salt: Vec<u8> = salt.iter().map(|b| b ^ 0x3a).collect();
            let _ = mac_salt;
        }
        v
    };
    let mut pages = Vec::with_capacity(raw.len() / PAGE_SIZE);
    for key in &candidates {
        pages.clear();
        let mut ok = true;
        for (i, chunk) in raw.chunks(PAGE_SIZE).enumerate() {
            let page_no = (i + 1) as u64;
            match decrypt_page(key, chunk, page_no) {
                Some(page) => pages.push(page),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            let page_count = pages.len();
            return Ok(DecryptedDb { pages, page_count });
        }
    }
    Err("表情数据库解密密钥无效（首次可能需要重新退出微信后重试获取）".into())
}

/// Collect favourite emoticon URLs by reading the decrypted pages as SQLite.
/// `conn` path uses the decrypted page cache written to a temp file.
pub fn export_urls(
    db_path: &PathBuf,
    key_material: &str,
    out_path: &PathBuf,
) -> Result<usize, String> {
    let decrypted = decrypt_db(db_path, key_material)?;
    let temp_dir = out_path
        .parent()
        .ok_or_else(|| "清单输出路径无效".to_string())?;
    fs::create_dir_all(temp_dir).map_err(|e| e.to_string())?;
    let plain = tempfile::NamedTempFile::new_in(temp_dir).map_err(|e| e.to_string())?;
    let plain_path = plain.path().to_path_buf();
    for page in &decrypted.pages {
        use std::io::Write;
        // Rewrite as contiguous plaintext SQLite image.
        // Restore reserve area with zeros to satisfy sqlite's page layout.
        let mut full = page.clone();
        full.extend_from_slice(&[0u8; RESERVE]);
        std::fs::OpenOptions::new()
            .append(true)
            .open(&plain_path)
            .map_err(|e| e.to_string())?
            .write_all(&full)
            .map_err(|e| e.to_string())?;
    }
    drop(plain);
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

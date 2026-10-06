fn main() {
    let w = std::env::args().nth(1).expect("usage: export-probe <wxid>");
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap();
    let db = home.join("Library/Containers/com.tencent.xinWeChat/Data/Documents/xwechat_files")
        .join(&w).join("db_storage/emoticon/emoticon.db");
    let out = home.join("Library/Caches/StickerNest/test-urls.txt");
    let key = stickernest_lib::wechat::cached_key(&w).unwrap().expect("no cached key");
    match stickernest_lib::wechat_db::export_urls(&db, &key, &out) {
        Ok(n) => { println!("exported {} urls -> {}", n, out.display()); }
        Err(e) => { eprintln!("err: {e}"); std::process::exit(1); }
    }
}

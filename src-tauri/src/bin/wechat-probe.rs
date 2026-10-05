// One-off probe: run the real WeChat dump key path end to end.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let wxid = args.get(1).cloned().expect("usage: wechat-probe <wxid>");
    let code = (|| {
        let accounts = stickernest_lib::wechat::detect_accounts()?;
        println!("accounts: {:?}", accounts.iter().map(|a| (a.wxid.clone(), a.has_db, a.has_cached_key)).collect::<Vec<_>>());
        match stickernest_lib::wechat::check_wechat_running() {
            stickernest_lib::wechat::WeChatRunning::NotRunning => println!("wechat: not running"),
            stickernest_lib::wechat::WeChatRunning::Running(n) => println!("wechat: running x{n}"),
        }
        let key = stickernest_lib::wechat::dump_key(&wxid, &mut |note| println!("stage: {note}"))?;
        println!("key captured, {} chars", key.trim().len());
        Ok::<(), String>(())
    })();
    match code {
        Ok(()) => (),
        Err(e) => { eprintln!("error: {e}"); std::process::exit(1); }
    }
}

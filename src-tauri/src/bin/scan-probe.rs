//! One-off probe: run the real scan against a library path and print JSON.
use stickernest_lib::library::Library;
fn main() {
    let path = std::env::args().nth(1).expect("usage: scan-probe LIBRARY_ROOT");
    let root = std::path::Path::new(&path);
    let library = Library::open(root).expect("open library");
    let management = library.get_management().expect("management");
    let grouped: std::collections::HashSet<String> = management
        .groups
        .iter()
        .flat_map(|g| g.member_ids.clone())
        .collect();
    let report = library
        .scan_static_duplicates(&management.ignored_pairs, &grouped)
        .expect("scan");
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

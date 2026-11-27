use std::fs::*;

fn load(path: &str) {
    // ruleid: rust/fs-read-unbounded-glob
    let text = read_to_string(path);
    // ruleid: rust/fs-read-unbounded-glob
    let bytes = read(path);
    let _ = text;
    let _ = bytes;
}

fn metadata_ok(path: &str) -> bool {
    let meta = metadata(path);
    let _ = meta;
    true
}

use std::fs as f;
use std::{fs as nested_fs};

fn load(path: &str) {
    // ruleid: rust/fs-read-unbounded
    let text = std::fs::read_to_string(path);
    // ruleid: rust/fs-read-unbounded
    let bytes = std::fs::read(path);
    // ruleid: rust/fs-read-unbounded
    let more = fs::read_to_string(path);
    let _ = text;
    let _ = bytes;
    let _ = more;
}

fn checked(path: &str) -> bool {
    let meta = std::fs::metadata(path);
    let _ = meta;
    true
}

fn alias_read(path: &str) {
    // ruleid: rust/fs-read-unbounded
    let text = f::read_to_string(path);
    let _ = text;
}

fn nested_alias_read(path: &str) {
    // ruleid: rust/fs-read-unbounded
    let text = nested_fs::read_to_string(path);
    let _ = text;
}

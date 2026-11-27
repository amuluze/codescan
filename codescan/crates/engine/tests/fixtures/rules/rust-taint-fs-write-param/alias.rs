use std::fs as f;

fn alias_save(user_input: &str) {
    // ruleid: rust/taint-fs-write-param-alias
    f::write("out.txt", user_input);
}

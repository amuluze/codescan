use std::{fs::*};

fn nested_glob_save(user_input: &str) {
    // ruleid: rust/taint-fs-write-param-alias
    write("out.txt", user_input);
}

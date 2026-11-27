use std::fs::*;

fn glob_save(user_input: &str) {
    // ruleid: rust/taint-fs-write-param-alias
    write("out.txt", user_input);
}

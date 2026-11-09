fn save(user_input: &str) {
    // ruleid: rust/taint-fs-write-param
    fs::write("out.txt", user_input);
}

fn save_literal() {
    fs::write("out.txt", "static");
}

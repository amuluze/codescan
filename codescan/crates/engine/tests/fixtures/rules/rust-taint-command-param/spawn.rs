fn run(user_input: String) {
    // ruleid: rust/taint-command-param
    Command::new(user_input);
}

fn sanitized(raw: String) {
    let n = raw.parse::<i32>().unwrap();
    Command::new(n.to_string());
}

fn literal() {
    Command::new("echo");
}

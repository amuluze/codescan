use std::process::Command as Cmd;

fn alias_run(user_input: String) {
    // ruleid: rust/taint-command-param-alias
    // ruleid: rust/process-command-new
    Cmd::new(user_input);
}

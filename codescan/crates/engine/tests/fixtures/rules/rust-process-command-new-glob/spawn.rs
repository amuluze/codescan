use std::process::*;

fn spawn() {
    // ruleid: rust/process-command-new-glob
    let c = Command::new("ls").output();
    let _ = c;
}

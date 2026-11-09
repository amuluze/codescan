use std::process::Command;

fn archive(dir: &str) {
    // ruleid: rust/process-command-new
    let out = std::process::Command::new("tar").arg(dir).output();
    // ruleid: rust/process-command-new
    let sp = process::Command::new("zip").spawn();
    let _ = out;
    let _ = sp;
}

fn local_alias() {
    // 裸 Command::new 无法与 clap::Command 等区分，不报
    let c = Command::new("ls").output();
    let _ = c;
}

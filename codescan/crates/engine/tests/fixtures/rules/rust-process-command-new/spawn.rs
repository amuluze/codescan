use std::process::Command;
use std::process::Command as Cmd;
use std::{process::Command as NestedCmd};

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

fn alias_spawn() {
    // ruleid: rust/process-command-new
    let c = Cmd::new("ls").output();
    let _ = c;
}

fn nested_alias_spawn() {
    // ruleid: rust/process-command-new
    let c = NestedCmd::new("ls").output();
    let _ = c;
}

fn helper(x: i32) -> i32 {
    // ruleid: rust/panic-in-library
    panic!("negative input");
}

fn classify(x: u8) -> u8 {
    match x {
        0 => x,
        // ruleid: rust/panic-in-library
        1 => todo!("not yet"),
        // ruleid: rust/panic-in-library
        _ => unreachable!("zero handled above"),
    }
}

fn main() {
    // main 入口内的失败退出不报
    panic!("bootstrap failed");
}

use std::mem::*;

fn cast(w: u32) -> i32 {
    // ruleid: rust/mem-transmute-glob
    let a = transmute::<u32, i32>(w);
    // ruleid: rust/mem-transmute-glob
    let b = transmute(w);
    let _ = a;
    let _ = b;
    0
}

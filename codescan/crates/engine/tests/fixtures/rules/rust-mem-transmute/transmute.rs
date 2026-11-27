use std::mem as m;
use std::{mem as nested_mem};

fn cast_bits(w: u32) -> i32 {
    // ruleid: rust/mem-transmute
    let u = std::mem::transmute::<u32, i32>(w);
    // ruleid: rust/mem-transmute
    let u2 = mem::transmute(w);
    let _ = u;
    let _ = u2;
    0
}

fn other_path(v: u32) -> u32 {
    // 非 std::mem 路径不报
    let other = crate::bits::transmute(v);
    let _ = other;
    v
}

fn alias_transmute(w: u32) -> i32 {
    // ruleid: rust/mem-transmute
    let u = m::transmute::<u32, i32>(w);
    u
}

fn nested_alias_transmute(w: u32) -> i32 {
    // ruleid: rust/mem-transmute
    let u = nested_mem::transmute::<u32, i32>(w);
    u
}

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

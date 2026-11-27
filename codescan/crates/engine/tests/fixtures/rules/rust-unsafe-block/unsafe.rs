fn read_raw(ptr: *const u8) -> u8 {
    // ruleid: rust/unsafe-block
    unsafe { *ptr }
}

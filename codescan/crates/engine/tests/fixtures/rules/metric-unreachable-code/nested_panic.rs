fn visit() {
    for _ in 0..1 {
        // ruleid: rust/panic-in-library
        unreachable!();
    }
    let _ = 1;
}

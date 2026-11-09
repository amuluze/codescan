struct Buffer;

fn drop_without_drop(v: Buffer) {
    // ruleid: rust/mem-forget
    std::mem::forget(v);
}

fn short_form(b: Buffer) {
    // ruleid: rust/mem-forget
    mem::forget(b);
}

fn inspect(b: Buffer) -> usize {
    let size = std::mem::size_of::<Buffer>();
    let _ = b;
    size
}

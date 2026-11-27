use std::mem as m;
use std::{mem as nested_mem};

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

fn alias_forget(b: Buffer) {
    // ruleid: rust/mem-forget
    m::forget(b);
}

fn nested_alias_forget(b: Buffer) {
    // ruleid: rust/mem-forget
    nested_mem::forget(b);
}

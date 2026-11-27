use std::mem::*;

struct Buffer;

fn drop_without_drop(v: Buffer) {
    // ruleid: rust/mem-forget-glob
    forget(v);
}

pub fn api_value() -> i32 {
    // ruleid: rust/unwrap-in-pub-fn
    let v = compute().unwrap();
    v
}

pub fn api_expect() -> i32 {
    // ruleid: rust/unwrap-in-pub-fn
    let w = compute().expect("msg");
    w
}

pub struct S;

impl S {
    pub fn m(&self) -> i32 {
        // ruleid: rust/unwrap-in-pub-fn
        self.inner().expect("bad")
    }
}

fn internal() -> i32 {
    // 私有函数内的解包不报
    let v = compute().unwrap();
    v
}

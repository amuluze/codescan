// 行尾注释不占语句序位：`return 1; // ...` 不得判为不可达代码。
fn trailing_comment_is_not_dead() -> i32 {
    return 1; // 行尾注释
}

// `return;` 之后的语句才是不可达代码。
fn dead_after_return() {
    return;
    // ruleid: metric/unreachable-code
    let value = 1;
}

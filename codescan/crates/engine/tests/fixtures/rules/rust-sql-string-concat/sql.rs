fn query_for(id: i64, key: &str, v: i32, k: i32) -> String {
    // ruleid: rust/sql-string-concat
    let sql = "SELECT * FROM users WHERE id = ".to_string() + &id.to_string();
    // ruleid: rust/sql-string-concat
    let frag = "DELETE FROM t WHERE k = " + key;
    // ruleid: rust/sql-string-concat
    let built = format!("UPDATE t SET v = {} WHERE k = {}", v, k);
    // ruleid: rust/sql-format-interpolation
    let named = format!("SELECT * FROM users WHERE id = {id}");
    let constant = format!("SELECT 1");
    let plain = "sum = " + "1";
    let _ = sql;
    let _ = frag;
    let _ = built;
    let _ = plain;
    String::new()
}

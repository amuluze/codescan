fn fetch(user_input: String) {
    // ruleid: rust/taint-sql-query-param
    db.query(user_input);
}

fn fetch_literal() {
    db.query("SELECT 1");
}

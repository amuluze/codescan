class QueryHandler {
  void run(String userInput) {
    // ruleid: java/taint-sql-param
    stmt.executeQuery("SELECT * FROM t WHERE c = " + userInput);
  }

  void sanitized(String raw) {
    int n = Integer.parseInt(raw);
    stmt.executeUpdate("UPDATE t SET c = " + String.valueOf(n));
  }

  void literal() {
    stmt.executeQuery("SELECT 1");
  }
}

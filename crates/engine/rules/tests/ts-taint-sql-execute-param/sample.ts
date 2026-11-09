function runQuery(userInput: string): void {
  // ruleid: ts/taint-sql-execute-param
  db.execute("SELECT * FROM t WHERE c = " + userInput);
}

function sanitizedQuery(raw: string): void {
  db.execute("SELECT * FROM t WHERE id = " + parseInt(raw, 10));
}

function staticQuery(): void {
  db.execute("SELECT 1");
}

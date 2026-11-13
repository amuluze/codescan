package sample

func query(userInput string) {
	// ruleid: go/taint-sql-param
	db.Query("SELECT * FROM t WHERE c = " + userInput)
}

func queryLiteral() {
	db.Query("SELECT 1")
}

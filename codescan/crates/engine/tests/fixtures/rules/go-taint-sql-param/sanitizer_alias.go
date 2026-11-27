package sample

import s "strconv"

func safeQuery(raw string) {
	n, err := s.Atoi(raw)
	if err != nil {
		return
	}
	db.Exec("DELETE FROM t WHERE id = ?", n)
}

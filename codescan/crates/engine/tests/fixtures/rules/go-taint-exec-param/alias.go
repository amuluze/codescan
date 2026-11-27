package sample

import (
	ex "os/exec"
	s "strconv"
)

func aliasExec(userInput string) {
	// ruleid: go/taint-exec-param-alias
	ex.Command(userInput)
}

func aliasSanitized(raw string) {
	n, err := s.Atoi(raw)
	if err != nil {
		return
	}
	ex.Command(s.Itoa(n))
}

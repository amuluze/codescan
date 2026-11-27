package sample

import (
	. "strconv"
	"os/exec"
)

func dotSanitized(raw string) {
	n, err := Atoi(raw)
	if err != nil {
		return
	}
	exec.Command(Itoa(n))
}

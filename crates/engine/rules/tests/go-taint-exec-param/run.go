package sample

import (
	"os/exec"
	"strconv"
)

func run(userInput string) {
	// ruleid: go/taint-exec-param
	exec.Command(userInput)
}

func sanitized(raw string) {
	n, err := strconv.Atoi(raw)
	if err != nil {
		return
	}
	exec.Command("echo", strconv.Itoa(n))
}

func literal() {
	exec.Command("echo", "ok")
}

package main

import (
	"os/exec"
)

func runSh(userCmd string) ([]byte, error) {
	// ruleid: go/exec-shell-command
	// ruleid: go/taint-exec-param
	out, err := exec.Command("sh", "-c", "echo "+userCmd).Output()
	return out, err
}

func runDirect(name string) ([]byte, error) {
	// ruleid: go/taint-exec-param
	out, err := exec.Command(name).Output()
	return out, err
}

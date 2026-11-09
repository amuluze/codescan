package main

import (
	"os/exec"
)

func runSh(userCmd string) ([]byte, error) {
	// ruleid: go/exec-shell-command
	out, err := exec.Command("sh", "-c", "echo "+userCmd).Output()
	return out, err
}

func runDirect(name string) ([]byte, error) {
	out, err := exec.Command(name).Output()
	return out, err
}

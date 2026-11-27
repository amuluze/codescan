package main

import ex "os/exec"

func runAlias() {
	// ruleid: go/exec-shell-command
	out, err := ex.Command("sh", "-c", "echo x").Output()
	_, _ = out, err
}

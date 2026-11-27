package main

import . "os/exec"

func dotRun() {
	// ruleid: go/exec-shell-command-dot
	out, err := Command("sh", "-c", "echo x").Output()
	_, _ = out, err
}

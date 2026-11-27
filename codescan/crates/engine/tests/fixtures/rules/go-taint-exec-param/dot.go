package sample

import . "os/exec"

func dotExec(userInput string) {
	// ruleid: go/taint-exec-param-alias
	Command(userInput)
}

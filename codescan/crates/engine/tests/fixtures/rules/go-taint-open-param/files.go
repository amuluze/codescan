package sample

import "os"

func open(path string) {
	// ruleid: go/taint-open-param
	os.Open(path)
}

func openLiteral() {
	os.Open("/etc/hostname")
}

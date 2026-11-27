package sample

import . "os"

func dotOpen(path string) {
	// ruleid: go/taint-open-param-alias
	Open(path)
}

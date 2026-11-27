package sample

import (
	f "path/filepath"
	o "os"
)

func aliasOpen(path string) {
	// ruleid: go/taint-open-param-alias
	o.Open(path)
}

func aliasClean(path string) {
	o.Open(f.Clean(path))
}

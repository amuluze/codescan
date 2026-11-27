package sample

import (
	. "path/filepath"
	"os"
)

func dotClean(path string) {
	os.Open(Clean(path))
}

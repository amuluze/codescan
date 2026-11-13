package main

import (
	"fmt"
	"os"
	"strconv"
)

func readConfig(path string) []byte {
	// ruleid: go/ignored-error
	data, _ := os.ReadFile(path)
	return data
}

var lastResult int

func apply(delta int) int {
	// ruleid: go/ignored-error
	lastResult, _ = bump(lastResult, delta)
	return lastResult
}

func bump(base, delta int) (int, error) {
	return base + delta, nil
}

func parse(s string) (int, string) {
	n, err := strconv.Atoi(s)
	if err != nil {
		return 0, err.Error()
	}
	return n, "ok"
}

func main() {
	cfg := readConfig("/etc/app.conf")
	value, status := parse(string(cfg))
	fmt.Println(value, status, apply(2))
}

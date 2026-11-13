package main

import (
	"math/rand"
	"strconv"
)

func verificationCode() string {
	// ruleid: go/insecure-random
	code := rand.Intn(900000)
	return strconv.Itoa(code)
}

func sessionId(b []byte) (int, error) {
	n, err := rand.Read(b)
	return n, err
}

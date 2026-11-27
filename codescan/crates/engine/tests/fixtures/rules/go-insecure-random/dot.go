package main

import . "math/rand"

func dotRandom() int {
	// ruleid: go/insecure-random-dot
	return Intn(10)
}

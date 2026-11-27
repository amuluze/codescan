package main

import r "math/rand"

func aliasRandom() int {
	// ruleid: go/insecure-random
	return r.Intn(10)
}

package main

import r2 "math/rand/v2"

func aliasRandomV2() int {
	// ruleid: go/insecure-random
	return r2.IntN(10)
}

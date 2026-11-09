package main

import (
	"math/rand/v2"
	"strconv"
)

func promoCode() string {
	// ruleid: go/insecure-random
	code := rand.IntN(900000)
	return strconv.Itoa(code)
}

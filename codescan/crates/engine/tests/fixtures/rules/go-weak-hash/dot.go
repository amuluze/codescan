package main

import . "crypto/md5"

func dotSum(data []byte) [16]byte {
	// ruleid: go/weak-hash-dot
	return Sum(data)
}

package main

import (
	"crypto/sha256"
	"crypto/md5"
	"crypto/sha1"
)

func checksum(data []byte) [16]byte {
	// ruleid: go/weak-hash
	sum := md5.Sum(data)
	return sum
}

func legacySum(data []byte) [20]byte {
	// ruleid: go/weak-hash
	sum := sha1.Sum(data)
	return sum
}

func strongSum(data []byte) [32]byte {
	sum := sha256.Sum256(data)
	return sum
}

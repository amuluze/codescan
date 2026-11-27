package main

import m "crypto/md5"

func aliasSum(data []byte) [16]byte {
	// ruleid: go/weak-hash
	return m.Sum(data)
}

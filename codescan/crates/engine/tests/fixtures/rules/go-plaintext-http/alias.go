package main

import h "net/http"

func aliasFetch(host string) (*h.Response, error) {
	// ruleid: go/plaintext-http-alias
	return h.Get("http://" + host)
}

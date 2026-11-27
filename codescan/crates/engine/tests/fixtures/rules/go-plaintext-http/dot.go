package main

import . "net/http"

func dotFetch(host string) (*Response, error) {
	// ruleid: go/plaintext-http-alias
	return Get("http://" + host)
}

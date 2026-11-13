package main

import (
	"net/http"
	"strings"
)

func fetch(host string) (*http.Response, error) {
	// ruleid: go/plaintext-http
	resp, err := http.Get("http://" + host + "/api/v1/items")
	return resp, err
}

func normalize(header string) string {
	return strings.ToLower(header)
}

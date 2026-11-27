package main

import (
	"net/http"
	"strings"
)

var globalURL = "http://localhost:8083/global"

func fetch(host string) (*http.Response, error) {
	// ruleid: go/plaintext-http
	resp, err := http.Get("http://" + host + "/api/v1/items")
	return resp, err
}

func fetchGlobal() (*http.Response, error) {
	// ruleid: go/plaintext-http
	resp, err := http.Get(globalURL)
	return resp, err
}

func fetchVar() (*http.Response, error) {
	url := "http://localhost:8080/var"
	// ruleid: go/plaintext-http
	resp, err := http.Get(url)
	return resp, err
}

func fetchConst() (*http.Response, error) {
	const url = "http://localhost:8081/const"
	// ruleid: go/plaintext-http
	resp, err := http.Get(url)
	return resp, err
}

func fetchVarDecl() (*http.Response, error) {
	var url = "http://localhost:8082/var-decl"
	// ruleid: go/plaintext-http
	resp, err := http.Get(url)
	return resp, err
}

func fetchTls(host string) (*http.Response, error) {
	return http.Get("https://" + host + "/api/v1/items")
}

func normalize(header string) string {
	return strings.ToLower(header)
}

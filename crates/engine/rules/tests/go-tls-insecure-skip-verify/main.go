package main

import (
	"crypto/tls"
	"net/http"
)

func insecureClient() *http.Client {
	cfg := &tls.Config{
		// ruleid: go/tls-insecure-skip-verify
		InsecureSkipVerify: true,
		MinVersion:         tls.VersionTLS12,
	}
	return &http.Client{Transport: cfg}
}

func strictClient() *http.Client {
	cfg := &tls.Config{InsecureSkipVerify: false}
	return &http.Client{Transport: cfg}
}

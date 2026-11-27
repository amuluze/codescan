package main

import (
	"crypto/tls"
	tc "crypto/tls"
	"net/http"
)

func insecureClient() *http.Client {
	// ruleid: go/tls-insecure-skip-verify
	cfg := &tls.Config{
		InsecureSkipVerify: true,
		MinVersion:         tls.VersionTLS12,
	}
	return &http.Client{Transport: cfg}
}

func strictClient() *http.Client {
	cfg := &tls.Config{InsecureSkipVerify: false}
	return &http.Client{Transport: cfg}
}

type customConfig struct {
	InsecureSkipVerify bool
}

func custom() customConfig {
	return customConfig{InsecureSkipVerify: true}
}

func aliasInsecureClient() *http.Client {
	// ruleid: go/tls-insecure-skip-verify
	cfg := &tc.Config{
		InsecureSkipVerify: true,
	}
	return &http.Client{Transport: cfg}
}

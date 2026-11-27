package main

import (
	// ruleid: go/unsafe-template
	"text/template"
)

func render(name, body string) (*template.Template, error) {
	t, err := template.New(name).Parse(body)
	return t, err
}

func escaped(s string) string {
	return template.HTMLEscapeString(s)
}

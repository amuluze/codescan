package main

import "text/template"

func render(name, body string) (*template.Template, error) {
	// ruleid: go/unsafe-template
	t, err := template.New(name).Parse(body)
	return t, err
}

func escaped(s string) string {
	return template.HTMLEscapeString(s)
}

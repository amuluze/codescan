package main

import "html/template"

func escaped(s string) string {
	return template.HTMLEscapeString(s)
}

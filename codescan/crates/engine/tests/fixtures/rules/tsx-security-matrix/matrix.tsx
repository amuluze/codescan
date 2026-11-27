function runSnippet(snippet: string): void {
  // ruleid: tsx/no-eval
  // ruleid: tsx/taint-eval-param
  eval(snippet);
  // ruleid: tsx/no-eval
  window.eval(snippet);
}

function make(): void {
  // ruleid: tsx/no-new-function
  const f = new Function("return 1");
  // ruleid: tsx/no-timer-string
  setTimeout("alert(1)", 0);
}

function render(value: string): void {
  // ruleid: tsx/no-inner-html
  // ruleid: tsx/taint-prompt-innerhtml
  document.body.innerHTML = prompt("x") ?? value;
}

function send(value: string): void {
  // ruleid: tsx/no-postmessage-wildcard
  window.postMessage(value, "*");
  window.postMessage("*", "https://example.com");
  // ruleid: tsx/no-document-write
  document.write("x");
  // ruleid: tsx/no-document-domain-assign
  document.domain = "example.com";
}

function sql(statement: string): void {
  // ruleid: tsx/taint-sql-execute-param
  db.execute(statement);
}

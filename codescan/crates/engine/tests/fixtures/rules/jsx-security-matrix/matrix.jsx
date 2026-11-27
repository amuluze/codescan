function runSnippet(snippet) {
  // ruleid: jsx/no-eval
  eval(snippet);
  // ruleid: jsx/no-eval
  window.eval(snippet);
}

function make() {
  // ruleid: jsx/no-new-function
  const f = new Function("return 1");
}

function render(value) {
  // ruleid: jsx/no-inner-html
  document.body.innerHTML = value;
}

function send(value) {
  // ruleid: jsx/no-postmessage-wildcard
  window.postMessage(value, "*");
  window.postMessage("*", "https://example.com");
  // ruleid: jsx/no-document-write
  document.write("x");
}

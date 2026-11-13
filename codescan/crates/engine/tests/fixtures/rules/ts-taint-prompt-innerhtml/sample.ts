function collect(): void {
  const answer = prompt("answer");
  // ruleid: ts/taint-prompt-innerhtml
  // ruleid: ts/no-inner-html
  el.innerHTML = answer;
}

function direct(): void {
  // ruleid: ts/taint-prompt-innerhtml
  // ruleid: ts/no-inner-html
  el.innerHTML = prompt("direct");
}

function staticOnly(): void {
  // ruleid: ts/no-inner-html
  el.innerHTML = "<b>ok</b>";
}

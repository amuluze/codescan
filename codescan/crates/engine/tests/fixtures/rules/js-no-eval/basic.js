// ruleid: js/no-eval
eval(userInput);

// ruleid: js/no-eval
window.eval(userInput);

function runSnippet(snippet) {
  // ruleid: js/no-eval
  return eval(snippet);
}

const kind = typeof eval;

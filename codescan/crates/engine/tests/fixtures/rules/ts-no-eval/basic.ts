// ruleid: ts/no-eval
eval(userInput);

function runSnippet(snippet: string): unknown {
  // ruleid: ts/no-eval
  // ruleid: ts/taint-eval-param
  return eval(snippet);
}

const kind = typeof eval;

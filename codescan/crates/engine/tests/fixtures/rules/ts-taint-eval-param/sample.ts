function handle(userInput: string): void {
  // ruleid: ts/taint-eval-param
  // ruleid: ts/no-eval
  eval(userInput);
}

function sanitizerCuts(raw: string): void {
  const n = parseInt(raw, 10);
  // ruleid: ts/no-eval
  eval(n);
}

function noChain(): void {
  // ruleid: ts/no-eval
  eval("1 + 1");
}

function unrelated(): void {
  // ruleid: ts/no-eval
  eval(externalCode);
}

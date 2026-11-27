function choose(a: any, b: any, cond: boolean): any {
  if (cond) {
    return a;
  }
  return b;
}

function run(input: string): void {
  const a: any = {};
  const b: any = {};
  const c = choose(a, b, false);
  c.value = input;
  // ruleid: ts/no-eval
  eval(a.value);
  // ruleid: ts/no-eval
  // ruleid: ts/taint-eval-param
  eval(b.value);
}

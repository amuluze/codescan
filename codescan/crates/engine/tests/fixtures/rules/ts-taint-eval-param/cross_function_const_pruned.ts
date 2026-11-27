function choose(a: any, b: any): any {
  const flag = true;
  return flag ? a : b;
}

function run(input: string): void {
  const a: any = {};
  const b: any = {};
  const c = choose(a, b);
  c.value = input;
  // ruleid: ts/no-eval
  // ruleid: ts/taint-eval-param
  eval(a.value);
  // ruleid: ts/no-eval
  eval(b.value);
}

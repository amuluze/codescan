function choose(a: any, b: any, flag: boolean): any {
  return flag ? a : b;
}

function run(input: string, flag: boolean): void {
  const a: any = {};
  const b: any = {};
  const c = choose(a, b, flag);
  c.value = input;
  // ruleid: ts/no-eval
  // ruleid: ts/taint-eval-param
  eval(a.value);
  // ruleid: ts/no-eval
  // ruleid: ts/taint-eval-param
  eval(b.value);
}

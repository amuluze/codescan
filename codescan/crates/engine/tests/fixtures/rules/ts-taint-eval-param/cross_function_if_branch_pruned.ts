function choose(a: any, b: any): any {
  let flag = true;
  if (true) {
    flag = false;
  }
  return flag ? a : b;
}

function run(input: string): void {
  const a: any = {};
  const b: any = {};
  const c = choose(a, b);
  c.value = input;
  // ruleid: ts/no-eval
  eval(a.value);
  // ruleid: ts/no-eval
  // ruleid: ts/taint-eval-param
  eval(b.value);
}

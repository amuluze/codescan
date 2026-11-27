function choose(a: any, b: any, cond: boolean): any {
  let flag;
  if (cond) {
    flag = true;
  } else {
    flag = false;
  }
  return flag ? a : b;
}

function run(input: string, cond: boolean): void {
  const a: any = {};
  const b: any = {};
  const c = choose(a, b, cond);
  c.value = input;
  // ruleid: ts/no-eval
  // ruleid: ts/taint-eval-param
  eval(a.value);
  // ruleid: ts/no-eval
  // ruleid: ts/taint-eval-param
  eval(b.value);
}

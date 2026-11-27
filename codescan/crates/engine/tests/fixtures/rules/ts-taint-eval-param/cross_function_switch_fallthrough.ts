function choose(a: any, b: any, n: number): any {
  let flag;
  switch (n) {
    case 1:
      flag = true;
    case 2:
      flag = false;
      break;
    default:
      flag = false;
  }
  return flag ? a : b;
}

function run(input: string): void {
  const a: any = {};
  const b: any = {};
  const c = choose(a, b, 1);
  c.value = input;
  // ruleid: ts/no-eval
  eval(a.value);
  // ruleid: ts/no-eval
  // ruleid: ts/taint-eval-param
  eval(b.value);
}

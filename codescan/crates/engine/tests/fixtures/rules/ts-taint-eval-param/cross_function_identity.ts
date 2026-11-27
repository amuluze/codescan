function id(o: any): any {
  return o;
}

function run(input: string): void {
  const a: any = {};
  const b = id(a);
  b.value = input;
  // ruleid: ts/no-eval
  // ruleid: ts/taint-eval-param
  eval(a.value);
}

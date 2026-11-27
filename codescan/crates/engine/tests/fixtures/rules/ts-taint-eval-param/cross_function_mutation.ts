function fill(o: any, v: string): void {
  o.value = v;
}

function run(input: string): void {
  const o: any = {};
  fill(o, input);
  // ruleid: ts/no-eval
  // ruleid: ts/taint-eval-param
  eval(o.value);
}

function maybe(o: any, v: string, cond: boolean): void {
  if (cond) {
    o.value = v;
  }
}

function run(input: string): void {
  const o: any = {};
  maybe(o, input, false);
  // ruleid: ts/no-eval
  eval(o.value);
}

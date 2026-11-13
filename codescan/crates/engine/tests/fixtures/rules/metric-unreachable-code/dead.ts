function pick(v: number): number {
  if (v > 0) {
    return 1;
    // ruleid: metric/unreachable-code
    v = 2;
  }
  return 0;
}

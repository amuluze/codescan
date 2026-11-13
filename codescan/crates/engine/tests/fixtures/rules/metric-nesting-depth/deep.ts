// ruleid: metric/nesting-depth
function deep(v: number): number {
  if (v > 1) {
    if (v > 2) {
      if (v > 3) {
        if (v > 4) {
          if (v > 5) {
            if (v > 6) {
              return 6;
            }
          }
        }
      }
    }
  }
  return 0;
}

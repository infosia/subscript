// corpus: trap/t07-p19-compound-assign-ordering // purpose: Traps before evaluating the right side of an out-of-bounds compound assignment. // exercises: Array, compound-assignment, evaluation-order, index-out-of-bounds // questions: none
function side(tag: i32): i32 {
  print(`side ${tag}`);
  return tag;
}

export function main(): void {
  const xs: i32[] = [1];
  print("before");
  xs[9] += side(2);
  print("after");
}

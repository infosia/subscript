// corpus: trap/t04-call-after-fault
// purpose: Stops before a later script-function call after an array bounds fault.
// exercises: Array, indexing, script call, post-fault function body
// questions: none
// expected-trap: index-out-of-bounds before the later script call

function later(): void {
  print("later start");
  print("later end");
}

export function main(): void {
  const values: i32[] = [7];
  print("before fault");
  const ignored: i32 = values[1];
  later();
  print(`after call ${ignored}`);
}

// corpus: trap/t06-push-after-fault
// purpose: Prevents array pushes and a length print after an array bounds fault.
// exercises: Array, indexing, push, length, post-fault state
// questions: none
// expected-trap: index-out-of-bounds before the array pushes

export function main(): void {
  const values: i32[] = [7];
  const pushed: i32[] = [];
  print("before fault");
  const ignored: i32 = values[1];
  pushed.push(ignored);
  pushed.push(2);
  print(`length ${pushed.length}`);
}

// corpus: trap/t05-foreach-callback-fault
// purpose: Stops inside an Array.forEach callback at an array bounds fault.
// exercises: Array, indexing, forEach, callback trap unwind
// questions: none
// expected-trap: index-out-of-bounds inside the forEach callback

export function main(): void {
  const values: i32[] = [1, 2, 3];
  const probe: i32[] = [7];
  print("before forEach");
  values.forEach((value: i32): void => {
    print(`callback ${value} before`);
    const ignored: i32 = probe[value];
    print(`callback ${ignored} after`);
  });
  print("after forEach");
}

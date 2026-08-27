// corpus: accept/a07-slice-pair
// purpose: Sums an f32 slice whose boundary lowers to a pointer-length pair.
// exercises: slice-parameter, pointer-length-lowering, tight-loop
// questions: Q1, Q4, Q12
// tsc: accepts; js-comparable: yes
function sum(values: f32[]): f32 {
  let total: f32 = 0.0;
  for (let index: i32 = 0; index < values.length; index += 1) {
    total += values[index];
  }
  return total;
}

export function main(): void {
  const values: f32[] = [1.25, 2.5, 5.0];
  print(`${sum(values)}`);
}

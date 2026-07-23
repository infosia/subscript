// benchmark: fib-loop
// Iterative Fibonacci in a tight inner loop, accumulated with i32
// two's-complement wrap. The inner seed depends (via a bitmask) on the running
// accumulator, a nonlinear loop-carried dependency that defeats the closed-form
// (scalar-evolution) folding a plain linear fib would allow. Both operands are
// masked to 10 bits so the feedback is identical under every language's i32/u32
// wrap. Checksum: the accumulated i32 sum (signed).

const INNER: i32 = 32;
const OUTER: i32 = 3000000;

export function main(): void {
  let result: i32 = 0;
  for (let iter: i32 = 0; iter < OUTER; iter += 1) {
    let a: i32 = iter & 1023;
    let b: i32 = 1 + (result & 1023);
    for (let i: i32 = 0; i < INNER; i += 1) {
      const t: i32 = a + b;
      a = b;
      b = t;
    }
    result += b;
  }
  print(`${result}`);
}

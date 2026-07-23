// benchmark: fib-recursive
// Naive recursive Fibonacci. fib(31) = 1346269.
// Checksum: fib(31) as i32.

function fib(n: i32): i32 {
  if (n < 2) {
    return n;
  }
  return fib(n - 1) + fib(n - 2);
}

export function main(): void {
  // The seed is read through the runtime array (opaque to the optimizer) so
  // fib is actually evaluated and not constant-folded to 1346269.
  const seed: i32[] = [31];
  const checksum: i32 = fib(seed[0]);
  print(`${checksum}`);
}

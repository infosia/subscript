// benchmark: primes
// Count primes up to 500000 by trial division (j*j <= n, no sqrt).
// Checksum: the prime count as i32.

const LIMIT: i32 = 500000;

function isPrime(n: i32): boolean {
  if (n < 2) {
    return false;
  }
  let j: i32 = 2;
  while (j * j <= n) {
    if (n % j === 0) {
      return false;
    }
    j += 1;
  }
  return true;
}

export function main(): void {
  let count: i32 = 0;
  for (let n: i32 = 2; n <= LIMIT; n += 1) {
    if (isPrime(n)) {
      count += 1;
    }
  }
  print(`${count}`);
}

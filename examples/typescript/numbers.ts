// Numbers. TypeScript has one number type, a 64-bit float. In subscript
// each number type has a size, and you name it. This script counts the
// coins and the prices in a small shop.

export function main(): void {
  // Write a sized type where TypeScript has `number`. `i32` is a 32-bit
  // integer, `u32` is a 32-bit integer that is never negative, `i64` is a
  // 64-bit integer, and `f64` is the float that JavaScript uses.
  const coins: i32 = 120;
  const shelves: u32 = 40;
  const sold: i64 = 9000000000;
  const price: f64 = 2.75;
  print(`coins ${coins}, shelves ${shelves}, sold ${sold}, price ${price}`);

  // A literal takes the type that you write. `7` below is an `i32`, so
  // `/` divides two integers and drops the remainder. node prints `3.5 1`.
  const apples: i32 = 7;
  print(`${apples / 2} ${apples % 2}`);

  // The same `7 / 2` with an `f64` type gives a float.
  const half: f64 = 7 / 2;
  print(`${half}`);

  // Two types do not mix by themselves. `as` converts the value: here an
  // `i32` becomes an `f64`. In TypeScript, `as` changes only the type that
  // the checker sees, and the value stays the same.
  const cost: f64 = (coins as f64) * price;
  print(`cost ${cost}`);

  // A conversion from `f64` to `i32` drops the fraction, as a C cast does.
  // node keeps the value and prints `9.99`.
  const paid: f64 = 9.99;
  print(`paid ${paid as i32}`);

  // An integer has a fixed size. One step past the largest value wraps
  // around to the smallest value. node prints `4294967296 2147483648`.
  const most: u32 = 4294967295;
  const plusOne: u32 = most + 1;
  const counter: i32 = 2147483647;
  print(`${plusOne} ${counter + 1}`);
}

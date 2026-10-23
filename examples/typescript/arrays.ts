// Arrays and arrow functions. Most array code reads the same as in
// TypeScript. The difference is in the arrow functions: an arrow that
// uses a local variable cannot leave its function. This script works
// with the scores of one game.

// This arrow uses no local variable, so the function can return it.
function makeDoubler(): (score: i32) => i32 {
  return (score: i32): i32 => score * 2;
}

export function main(): void {
  // An array has one element type. `push` adds an element at the end.
  const scores: i32[] = [30, 10, 20];
  scores.push(50);
  print(`${scores.join(",")} has ${scores.length} scores`);

  // `map`, `filter`, and `reduce` work as in TypeScript. Give each arrow
  // its parameter types and its result type.
  const bonus: i32 = 5;
  const raised: i32[] = scores.map((score: i32): i32 => score + bonus);
  const high: i32[] = raised.filter((score: i32): boolean => score >= 30);
  const total: i32 = high.reduce((sum: i32, score: i32): i32 => sum + score, 0);
  print(`raised ${raised.join(",")}`);
  print(`high ${high.join(",")}, total ${total}`);

  // The arrow in `map` above uses the local `bonus`. That is correct,
  // because `map` calls the arrow before `main` returns. A function that
  // returns such an arrow does not compile: the arrow keeps `bonus` in
  // the memory of the function, and that memory ends at the return.
  // TypeScript accepts it.
  print(`doubled ${scores.map(makeDoubler()).join(",")}`);

  // `indexOf` and `includes` work as in TypeScript.
  print(`${scores.indexOf(20)} ${scores.includes(99)}`);

  // An index must be inside the array. A read outside it stops the
  // program; node gives `undefined`. Test the length first.
  const i: i32 = 7;
  if (i < scores.length) {
    print(`score ${scores[i]}`);
  } else {
    print(`no score at ${i}`);
  }
}

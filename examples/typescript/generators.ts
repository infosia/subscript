// Generators. A `function*` stops at each `yield` and goes on at the next
// call of `next()`. subscript has no event loop and no timer, so this is
// how a script does work over several frames. This script opens a door.

// The door opens a little on each frame. The function keeps `angle`
// between two calls of `next()`.
function* openDoor(steps: i32): Generator<i32> {
  let angle: i32 = 0;
  for (let step: i32 = 0; step < steps; step += 1) {
    angle += 30;
    yield angle;
  }
  print("door open");
}

export function main(): void {
  // In a game, the host calls the script once per frame, and the script
  // calls `next()` once. Here a loop plays the part of the frames.
  const door: Generator<i32> = openDoor(3);
  let frame: i32 = 1;
  let result = door.next();
  while (!result.done) {
    print(`frame ${frame}: angle ${result.value}`);
    frame += 1;
    result = door.next();
  }

  // `for...of` runs a generator to its end, as in TypeScript.
  let total: i32 = 0;
  for (const angle of openDoor(2)) {
    total += angle;
  }
  print(`total ${total}`);
}

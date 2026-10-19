// Classes. A subscript class has a name that counts: two classes with the
// same fields are two different types. A class is also one of two kinds,
// a reference or a value. This script moves a player on a small map.

// Two classes with the same fields. TypeScript compares the fields, so it
// accepts a `Seconds` where a `Meters` is declared. subscript compares the
// class names, so `walk(new Seconds(3.0))` does not compile.
class Meters {
  value: f64;
  constructor(value: f64) {
    this.value = value;
  }
}

class Seconds {
  value: f64;
  constructor(value: f64) {
    this.value = value;
  }
}

function walk(distance: Meters): string {
  return `walk ${distance.value} m`;
}

// `@ValueType` makes a value class. A value class is copied when you
// assign it and when you pass it to a function, as a number is. It has
// the memory layout of a C struct.
@ValueType
class Position {
  x: i32;
  y: i32;
  constructor(x: i32, y: i32) {
    this.x = x;
    this.y = y;
  }
}

// A class without `@ValueType` is a reference class, as in TypeScript.
// An assignment copies the reference, so two names see one object.
class Player {
  name: string;
  position: Position;
  constructor(name: string, position: Position) {
    this.name = name;
    this.position = position;
  }
}

// The function gets its own copy of the position. The change below does
// not reach the caller.
function stepRight(position: Position): i32 {
  position.x += 1;
  return position.x;
}

export function main(): void {
  print(walk(new Meters(3.0)));
  const time = new Seconds(1.5);
  print(`time ${time.value} s`);

  // Assignment copies a value class. node prints `start 9,0` and `copy 9,0`,
  // because a JavaScript class is always a reference.
  const start = new Position(0, 0);
  const copy = start;
  copy.x = 9;
  print(`start ${start.x},${start.y}`);
  print(`copy ${copy.x},${copy.y}`);

  // A call copies a value class too. node prints `inside 10, outside 10`,
  // because there `start` and `copy` are one object with `x` at 9.
  const inside = stepRight(start);
  print(`inside ${inside}, outside ${start.x}`);

  // A reference class is shared. Both names see the change.
  const ada = new Player("ada", start);
  const same = ada;
  same.position = new Position(5, 5);
  print(`${ada.name} at ${ada.position.x},${ada.position.y}`);
}

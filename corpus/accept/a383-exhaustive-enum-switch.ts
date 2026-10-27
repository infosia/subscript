// corpus: accept/a383-exhaustive-enum-switch
// purpose: A function that returns from a case for each enum member needs no return after the switch.
// exercises: enum, switch, return-flow
// questions: compiler.md §200
// tsc: accepts; js-comparable: yes
enum Shape {
  Circle,
  Square,
  Triangle,
}

enum Level {
  Low = 1,
  High = 10,
}

function sides(s: Shape): i32 {
  switch (s) {
    case Shape.Circle:
      return 0;
    case Shape.Square:
      return 4;
    case Shape.Triangle:
      return 3;
  }
}

function name(l: Level): string {
  switch (l) {
    case Level.Low:
    case Level.High:
      return l === Level.Low ? "low" : "high";
  }
}

class Tile {
  shape: Shape;
  constructor(shape: Shape) {
    this.shape = shape;
  }
  area(size: f64): f64 {
    switch (this.shape) {
      case Shape.Circle: {
        return 3.0 * size * size;
      }
      case Shape.Square:
        return size * size;
      case Shape.Triangle:
        return size * size / 2.0;
    }
  }
}

export function main(): void {
  print(`${sides(Shape.Circle)} ${sides(Shape.Square)} ${sides(Shape.Triangle)}`);
  print(`${name(Level.Low)} ${name(Level.High)}`);
  print(`${new Tile(Shape.Square).area(3.0)} ${new Tile(Shape.Triangle).area(2.0)}`);
}

// corpus: accept/a386-transparent-type-alias/lib
// purpose: Exports a transparent alias of a class.
// exercises: type-alias, module-export
// questions: compiler.md §202
// tsc: accepts; js-comparable: yes
export class Point {
  x: f64;
  y: f64;
  constructor(x: f64, y: f64) {
    this.x = x;
    this.y = y;
  }
}
export type P = Point;
export function origin(): P {
  return new Point(0, 0);
}

// corpus: accept/a394-interface-constraint/shapes
// purpose: Exports an interface and two classes that declare it.
// exercises: interface, implements, module-export
// questions: compiler.md §208
// tsc: accepts; js-comparable: yes
export interface Shape {
  area(): f64;
  scaled(k: f64): f64;
  get name(): string;
}

export interface Tagged {
  tag(): i32;
}

export class Square implements Shape, Tagged {
  side: f64;
  constructor(side: f64) {
    this.side = side;
  }
  area(): f64 {
    return this.side * this.side;
  }
  scaled(k: f64): f64 {
    return this.area() * k;
  }
  get name(): string {
    return "square";
  }
  tag(): i32 {
    return 4;
  }
}

export class Circle implements Shape {
  r: f64;
  constructor(r: f64) {
    this.r = r;
  }
  area(): f64 {
    return 3 * this.r * this.r;
  }
  scaled(k: f64): f64 {
    return this.area() * k;
  }
  get name(): string {
    return "circle";
  }
}

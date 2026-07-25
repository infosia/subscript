// Generates corpus/accept/a69-json-stringify.expected.
// Required oracle: node v24.18.0.

const fs = require("node:fs");

class Point {
  constructor(x, ready) {
    this.x = x;
    this.ready = ready;
  }
}

class Person {
  constructor(name, age, active) {
    this.name = name;
    this.age = age;
    this.active = active;
  }
}

const values = [
  -8,
  250,
  -1600,
  65000,
  -2000000000,
  4000000000,
  -9007199254740991,
  9007199254740991,
  Math.fround(1.5),
  0.000001,
  -0,
  true,
  false,
  String.fromCharCode(...Array.from({ length: 32 }, (_, index) => index)) +
    " \"/\\" +
    String.fromCharCode(0x7f, 0x80, 0x2028, 0x2029),
  new Date(0),
  [[1, 2], [], [-3, 4]],
  [-2, 0, 7],
  new Point(9, true),
];

const person = new Person("Ada", 37, false);
values.push(person, person, null);

const output = values.map((value) => JSON.stringify(value)).join("\n") + "\n";
const destination = process.argv[2];
if (!destination) {
  process.stdout.write(output);
} else {
  fs.writeFileSync(destination, output);
}

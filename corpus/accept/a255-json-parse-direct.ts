// corpus: accept/a255-json-parse-direct
// purpose: Parses each target family directly into T, catches SyntaxError with its byte offset and the depth limit, catches TypeError on each mismatch class, and parses again after the failures.
// exercises: JSON, typed parse, try-catch, SyntaxError, TypeError, byte offset, depth limit, no partial value
// questions: Q28, Q9, Q5
// tsc: accepts; js-comparable: no C2 C6: The parse failure messages are this project's.
@ValueType
class Point {
  x: i32;
  ready: boolean;

  constructor(x: i32, ready: boolean) {
    this.x = x;
    this.ready = ready;
  }
}

class Person {
  name: string;
  age: u8;
  friend: Person | null;

  constructor(name: string, age: u8, friend: Person | null) {
    this.name = name;
    this.age = age;
    this.friend = friend;
  }
}

class Config {
  name: string;
  count: i32;

  constructor(name: string, count: i32) {
    this.name = name;
    this.count = count;
  }
}

function show(label: string, error: Error): void {
  print(`${label}: ${error.name}: ${error.message}`);
}

function parseConfig(label: string, text: string): void {
  try {
    const config: Config = JSON.parse(text);
    print(`${label}: ok ${config.name}:${config.count}`);
  } catch (e) {
    if (e instanceof Error) {
      show(label, e);
    }
  }
}

function parseNumbers(label: string, text: string): void {
  try {
    const values: i32[] = JSON.parse<i32[]>(text);
    print(`${label}: ok ${values.length}`);
  } catch (e) {
    if (e instanceof Error) {
      show(label, e);
    }
  }
}

export function main(): void {
  const count: i32 = JSON.parse("42");
  print(`i32: ${count}`);
  print(`u64: ${JSON.parse<u64>("18446744073709551615")}`);
  print(`f64: ${JSON.parse<f64>("-0.5")}`);
  print(`f32: ${JSON.parse<f32>("0.25")}`);
  print(`boolean: ${JSON.parse<boolean>("true")}`);
  print(`string: ${JSON.parse<string>('"caf\\u00e9 \\ud83d\\ude00"')}`);
  const fixed: FixedArray<i16, 3> = JSON.parse("[1,-2,3]");
  print(`FixedArray: ${fixed[0]} ${fixed[1]} ${fixed[2]}`);
  const point: Point = JSON.parse<Point>('{"x":3,"ready":true}');
  print(`ValueType: ${point.x} ${point.ready}`);
  const person: Person = JSON.parse<Person>(
    '{"name":"ada","age":36,"friend":{"name":"bob","age":7,"friend":null}}',
  );
  const friend: Person | null = person.friend;
  if (friend !== null) {
    print(`class: ${person.name} ${person.age} ${friend.name} ${friend.age}`);
  }
  const nobody: Person | null = JSON.parse("null");
  print(`nullable: ${nobody === null}`);
  parseNumbers("array", "[1,2,3]");
  parseConfig("class", '{"name":"demo","count":3}');

  parseConfig("truncated", '{"name":"demo","count":');
  parseNumbers("trailing", "[1] x");
  parseNumbers("utf8 offset", '["é",?]');
  parseNumbers("empty", "");
  parseNumbers("depth", "[".repeat(129) + "0" + "]".repeat(129));
  parseNumbers("depth 128", "[".repeat(128) + "0" + "]".repeat(128));

  parseConfig("missing field", '{"name":"demo"}');
  parseConfig("wrong kind", '{"name":"demo","count":"three"}');
  parseConfig("contextual target", "[]");
  parseNumbers("element", '[1,2,"three"]');
  try {
    print(`${JSON.parse<u8>("256")}`);
  } catch (e) {
    if (e instanceof TypeError) {
      show("out of range", e);
    }
  }
  try {
    print(`${JSON.parse<i32>("1.5")}`);
  } catch (e) {
    if (e instanceof TypeError) {
      show("not an integer", e);
    }
  }
  try {
    print(`${JSON.parse<f32>("1e39")}`);
  } catch (e) {
    if (e instanceof TypeError) {
      show("f32 overflow", e);
    }
  }
  try {
    print(JSON.parse<string>('"\\ud800"'));
  } catch (e) {
    if (e instanceof TypeError) {
      show("lone surrogate", e);
    }
  }
  try {
    const short: FixedArray<i16, 3> = JSON.parse<FixedArray<i16, 3>>("[1,2]");
    print(`${short[0]}`);
  } catch (e) {
    if (e instanceof TypeError) {
      show("fixed length", e);
    }
  }

  let kept: Config = new Config("before", 1);
  try {
    kept = JSON.parse<Config>('{"name":"after"}');
  } catch {
    print("no partial value");
  }
  print(`kept: ${kept.name}:${kept.count}`);

  parseConfig("after failures", '{"name":"again","count":4}');
}

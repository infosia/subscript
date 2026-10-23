// corpus: accept/a372-json-mismatch-path
// purpose: Reports the first mismatch in declaration and index order, then parses a valid document.
// exercises: JSON, mismatch path, nullable, FixedArray, duplicate key, recovery
// questions: Q28, Q9
// tsc: accepts; js-comparable: no C6: The parse failure messages are this project's.
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
  values: FixedArray<i16, 3>;
  constructor(values: FixedArray<i16, 3>) { this.values = values; }
}
function person(label: string, text: string): void {
  try { JSON.parse<Person>(text); }
  catch (e) { if (e instanceof TypeError) { print(label + ": " + e.message); } }
  const valid: Person = JSON.parse<Person>('{"name":"ok","age":1,"friend":null}');
  print("after " + label + ": " + valid.name);
}
export function main(): void {
  person("nested range", '{"name":"a","age":1,"friend":{"name":"b","age":300,"friend":null}}');
  person("nullable kind", '{"name":"a","age":1,"friend":5}');
  person("nested missing", '{"name":"a","age":1,"friend":{"name":"b","friend":null}}');
  person("first field", '{"age":300,"name":7,"friend":null}');
  person("duplicate key", '{"name":"a","age":1,"age":300,"friend":null}');
  try { JSON.parse<Person[]>('[{"name":"a","age":1,"friend":null},{"name":5,"age":1,"friend":null}]'); }
  catch (e) { if (e instanceof TypeError) { print("class element: " + e.message); } }
  const people: Person[] = JSON.parse<Person[]>('[{"name":"ok","age":1,"friend":null}]');
  print("after class element: " + people[0].name);
  try { JSON.parse<Config>('{"values":[1,2]}'); }
  catch (e) { if (e instanceof TypeError) { print("fixed field: " + e.message); } }
  const config: Config = JSON.parse<Config>('{"values":[1,2,3]}');
  print(`after fixed field: ${config.values[2]}`);
}

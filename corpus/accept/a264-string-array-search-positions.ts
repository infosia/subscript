// corpus: accept/a264-string-array-search-positions
// purpose: Search positions, split limits, and aliases (stdlib.md §8.9 and §9.9).
// exercises: string, array, regexp
// questions: Q21, Q22
// tsc: accepts; js-comparable: yes
export function main(): void {
  const text = "abcabc";
  print(`${text.lastIndexOf("c", 3)} ${text.lastIndexOf("c", 100)} ${text.lastIndexOf("c", -5)}`);
  print(`${text.lastIndexOf("a", 0)} ${text.lastIndexOf("", 2)} ${text.lastIndexOf("", 99)}`);
  const values: i32[] = [1, 2, 3, 1, 2];
  print(`${values.indexOf(1, 1)} ${values.indexOf(1, -2)} ${values.indexOf(1, -100)} ${values.indexOf(2, 99)}`);
  const floats: f64[] = [NaN, 1];
  print(`${floats.includes(NaN, 0)} ${floats.includes(NaN, 1)}`);
  print(`${values.lastIndexOf(1, 2)} ${values.lastIndexOf(1, -3)} ${values.lastIndexOf(1, -100)} ${values.lastIndexOf(2, 99)}`);
  const spliced: i32[] = [1, 2, 3, 4, 5];
  print(JSON.stringify(spliced.splice(2)));
  print(JSON.stringify(spliced));
  print(JSON.stringify([1, 2, 3].splice(-1)));
  print(JSON.stringify([1, 2].splice(9)));
  print(JSON.stringify("a,b,c".split(",", 2)));
  print(JSON.stringify("a,b,c".split(",", 0)));
  print(JSON.stringify("a,b,c".split(",", -1)));
  print(JSON.stringify("a1b2c".split(/\d/, 1)));
  print(JSON.stringify("abc".split("", 2)));
  print([1, 2].toString());
  print(`[${"  x  ".trimLeft()}] [${"  x  ".trimRight()}]`);
  print(`${text.lastIndexOf("c")} ${values.indexOf(1)} ${values.lastIndexOf(1)} ${values.includes(3)}`);
  print(JSON.stringify("a1b2c".split(/\d/, 0)));
  print(JSON.stringify("a1b2c".split(/\d/, -1)));
  print(JSON.stringify("a1b2c".split(/(\d)/, 2)));
  print(`${[true, false].toString()} ${["a", "b"].toString()}`);
}

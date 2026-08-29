// corpus: accept/a171-static-array-callbacks
// purpose: Known callbacks and function values produce the same Array method results.
// observable: All callback methods preserve results, index arguments, and short-circuit order.
// exercises: static-array-callback-loop, direct-callback-call, function-value-intrinsic
// questions: Q22, Q27
// tsc: accepts; js-comparable: yes
function mapNamed(value: i32, index: i32): i32 {
  return value + index;
}

function filterNamed(value: i32, index: i32): boolean {
  return value + index >= 7;
}

function reduceNamed(acc: i32, value: i32, index: i32): i32 {
  return acc * 10 + value + index;
}

function forEachNamed(value: i32, index: i32): void {
  print(`forEach:${value}:${index}`);
}

function someNamed(value: i32, index: i32): boolean {
  print(`some:${value}:${index}`);
  return value + index >= 8;
}

function everyNamed(value: i32, index: i32): boolean {
  print(`every:${value}:${index}`);
  return value + index < 10;
}

function findNamed(value: i32, index: i32): boolean {
  return value + index === 8;
}

function missNamed(_value: i32, _index: i32): boolean {
  return false;
}

function runNamed(values: i32[]): void {
  print("named");
  print(`map:${values.map(mapNamed).join(",")}`);
  print(`filter:${values.filter(filterNamed).join(",")}`);
  print(`reduce:${values.reduce(reduceNamed, 1)}`);
  print(`reduceRight:${values.reduceRight(reduceNamed, 1)}`);
  values.forEach(forEachNamed);
  print(`some-result:${values.some(someNamed)}`);
  print(`every-result:${values.every(everyNamed)}`);
  print(`find:${values.findIndex(findNamed)}`);
  print(`miss:${values.findIndex(missNamed)}`);
}

function runLambda(values: i32[]): void {
  const offset: i32 = 0;
  print("lambda");
  print(
    `map:${values.map((value: i32, index: i32): i32 => value + index + offset).join(",")}`,
  );
  print(
    `filter:${values.filter((value: i32, index: i32): boolean => value + index >= 7).join(",")}`,
  );
  print(
    `reduce:${values.reduce((acc: i32, value: i32, index: i32): i32 => acc * 10 + value + index, 1)}`,
  );
  print(
    `reduceRight:${values.reduceRight((acc: i32, value: i32, index: i32): i32 => acc * 10 + value + index, 1)}`,
  );
  values.forEach((value: i32, index: i32): void => {
    print(`forEach:${value}:${index}`);
  });
  print(
    `some-result:${values.some((value: i32, index: i32): boolean => {
      print(`some:${value}:${index}`);
      return value + index >= 8;
    })}`,
  );
  print(
    `every-result:${values.every((value: i32, index: i32): boolean => {
      print(`every:${value}:${index}`);
      return value + index < 10;
    })}`,
  );
  print(
    `find:${values.findIndex((value: i32, index: i32): boolean => value + index === 8)}`,
  );
  print(
    `miss:${values.findIndex((_value: i32, _index: i32): boolean => false)}`,
  );
}

function runValue(values: i32[]): void {
  const mapValue: (value: i32, index: i32) => i32 = mapNamed;
  const filterValue: (value: i32, index: i32) => boolean = filterNamed;
  const reduceValue: (acc: i32, value: i32, index: i32) => i32 = reduceNamed;
  const forEachValue: (value: i32, index: i32) => void = forEachNamed;
  const someValue: (value: i32, index: i32) => boolean = someNamed;
  const everyValue: (value: i32, index: i32) => boolean = everyNamed;
  const findValue: (value: i32, index: i32) => boolean = findNamed;
  const missValue: (value: i32, index: i32) => boolean = missNamed;
  print("value");
  print(`map:${values.map(mapValue).join(",")}`);
  print(`filter:${values.filter(filterValue).join(",")}`);
  print(`reduce:${values.reduce(reduceValue, 1)}`);
  print(`reduceRight:${values.reduceRight(reduceValue, 1)}`);
  values.forEach(forEachValue);
  print(`some-result:${values.some(someValue)}`);
  print(`every-result:${values.every(everyValue)}`);
  print(`find:${values.findIndex(findValue)}`);
  print(`miss:${values.findIndex(missValue)}`);
}

export function main(): void {
  const values: i32[] = [2, 4, 6, 8];
  runNamed(values);
  runLambda(values);
  runValue(values);
}

// corpus: accept/a68-q27-fixed-array-callbacks
// purpose: Exercises both callback arities for the Q27 FixedArray family.
//          Every indexed callback makes its index observable in output.
// exercises: fixed-array-callbacks, dual-callback-arity, dynamic-results
// questions: Q27, C5

let valueTrace: string = "";
let indexedTrace: string = "";

function indexedMap(value: i32, index: i32): string {
  return `${index}:${value + index}`;
}

export function main(): void {
  const values: FixedArray<i32, 4> = [4, 7, 10, 13];

  valueTrace = "";
  values.forEach((value: i32): void => {
    valueTrace += `${value}|`;
  });
  indexedTrace = "";
  values.forEach((value: i32, index: i32): void => {
    indexedTrace += `${index}:${value}|`;
  });
  print(`forEach ${valueTrace} ${indexedTrace}`);

  const mappedValue: i32[] = values.map((value: i32): i32 => value * 2);
  const mappedIndex: string[] = values.map(indexedMap);
  print(`map ${mappedValue.join(",")} ${mappedIndex.join(",")}`);

  const filteredValue: i32[] = values.filter((value: i32): boolean => value % 2 === 0);
  const filteredIndex: i32[] = values.filter(
    (value: i32, index: i32): boolean => value > 0 && index % 2 === 1,
  );
  print(`filter ${filteredValue.join(",")} ${filteredIndex.join(",")}`);

  const someValue: boolean = values.some((value: i32): boolean => value > 12);
  const someIndex: boolean = values.some(
    (value: i32, index: i32): boolean => index === 2 && value === 10,
  );
  print(`some ${someValue} ${someIndex}`);

  const everyValue: boolean = values.every((value: i32): boolean => value > 0);
  const everyIndex: boolean = values.every(
    (value: i32, index: i32): boolean => value === 4 + index * 3,
  );
  print(`every ${everyValue} ${everyIndex}`);

  const foundValue: i32 = values.findIndex((value: i32): boolean => value === 10);
  const foundIndex: i32 = values.findIndex(
    (value: i32, index: i32): boolean => index === 3 && value === 13,
  );
  print(`findIndex ${foundValue} ${foundIndex}`);

  const reducedValue: i32 = values.reduce(
    (acc: i32, value: i32): i32 => acc + value,
    0,
  );
  const reducedIndex: i32 = values.reduce(
    (acc: i32, value: i32, index: i32): i32 => acc + value * index,
    0,
  );
  print(`reduce ${reducedValue} ${reducedIndex}`);

  const rightValue: string = values.reduceRight(
    (acc: string, value: i32): string =>
      acc.length === 0 ? `${value}` : `${acc},${value}`,
    "",
  );
  const rightIndex: string = values.reduceRight(
    (acc: string, value: i32, index: i32): string =>
      acc.length === 0 ? `${index}:${value}` : `${acc},${index}:${value}`,
    "",
  );
  print(`reduceRight ${rightValue} ${rightIndex}`);
}

// corpus: accept/a214-set-source-battery
// purpose: Constructs a Set from each accepted source and pins the order.
// exercises: set-source-construction, fused-traversal, same-value-zero
// questions: Q24, Q30, compiler section 103
// tsc: accepts; js-comparable: yes
function render(values: Set<i32>): string {
  let out: string = "";
  for (const value of values) {
    out += `${value},`;
  }
  return `${out}size=${values.size}`;
}

export function main(): void {
  const fromArray: Set<i32> = new Set<i32>([3, 1, 3, 2]);
  print(`array ${render(fromArray)}`);

  const fixed: FixedArray<i32, 4> = [5, 4, 5, 6];
  const fromFixed: Set<i32> = new Set<i32>(fixed);
  print(`fixed ${render(fromFixed)}`);

  const fromSet: Set<i32> = new Set<i32>(fromArray);
  print(`set ${render(fromSet)}`);

  const none: i32[] = [];
  const empty: Set<i32> = new Set<i32>(none);
  print(`empty ${render(empty)}`);

  const fromString: Set<string> = new Set<string>("banana漢字漢");
  let text: string = "";
  for (const codePoint of fromString) {
    text += `${codePoint},`;
  }
  print(`string ${text}size=${fromString.size}`);

  const floats: Set<f64> = new Set<f64>([0.0, -0.0, NaN, NaN, 1.5, 1.5]);
  let numbers: string = "";
  for (const value of floats) {
    numbers += `${value},`;
  }
  print(`floats ${numbers}size=${floats.size}`);
  print(`has-zero ${floats.has(-0.0)} has-nan ${floats.has(NaN)}`);

  const negativeFirst: Set<f64> = new Set<f64>([-0.0, 0.0]);
  let signs: string = "";
  for (const value of negativeFirst) {
    signs += `${1.0 / value},`;
  }
  print(`neg-zero-first ${signs}size=${negativeFirst.size}`);
}

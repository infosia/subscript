// corpus: accept/a512-in-place-filter
// purpose: A filter of a new array with one reader gives the output of a filter that allocates.
// exercises: filter, map, array-literal, lambda, capture, index-parameter, exception, loop
// questions: compiler.md §359, §8.1d
// tsc: accepts; js-comparable: yes

class Point {
  x: i32;
  constructor(x: i32) {
    this.x = x;
  }
}

function isEven(value: i32): boolean {
  return value % 2 === 0;
}

function keepParameter(values: i32[]): string {
  const kept: i32[] = values.filter(isEven);
  return `${kept.join(",")} ${values.join(",")}`;
}

export function main(): void {
  const xs: i32[] = [1, 2, 3, 4, 5, 6, 7, 8];

  // Form 2a after a map, after a filter, and after an array literal.
  print(xs.map((x: i32): i32 => x * 3).filter(isEven).join(","));
  print(xs.filter((x: i32): boolean => x > 2).filter((x: i32): boolean => x < 7).join(","));
  const positions: i32[] = [];
  const fromLiteral: i32[] = [10, 11, 12, 13, 14].filter((x: i32, i: i32): boolean => {
    positions.push(i);
    return x % 2 === 1;
  });
  print(`${fromLiteral.join(",")} ${positions.join(",")}`);

  // Form 2b, with the index parameter: the index is the input position.
  const doubled: i32[] = xs.map((x: i32): i32 => x * 2);
  const seen: string[] = [];
  const odd: i32[] = doubled.filter((x: i32, i: i32): boolean => {
    seen.push(`${x}@${i}`);
    return i % 2 === 1;
  });
  print(`${odd.join(",")} ${seen.join(" ")}`);

  // Element types f64, string, and a reference class; a callback that allocates.
  const halves: f64[] = xs.map((x: i32): f64 => (x as f64) * 0.5);
  print(halves.filter((h: f64): boolean => h !== Math.floor(h)).join(","));
  const words: string[] = xs.map((x: i32): string => `w${x * 7}`);
  const longWords: string[] = words.filter((w: string): boolean => `${w}!`.length > 3);
  print(longWords.join(","));
  const points: Point[] = xs.map((x: i32): Point => new Point(x * x));
  const farPoints: Point[] = points.filter((p: Point): boolean => {
    const probe: Point = new Point(p.x + 1);
    return probe.x > 10;
  });
  print(farPoints.map((p: Point): string => `${p.x}`).join(","));

  // A lambda with a capture.
  const limit: i32 = 5;
  print(xs.map((x: i32): i32 => x + 1).filter((x: i32): boolean => x <= limit).join(","));

  // A filter that keeps each element, one that keeps none, and an empty receiver.
  const all: i32[] = xs.map((x: i32): i32 => x).filter((x: i32): boolean => x > 0);
  const none: i32[] = xs.map((x: i32): i32 => x).filter((x: i32): boolean => x > 100);
  const empty: i32[] = [];
  const fromEmpty: i32[] = empty.filter((x: i32): boolean => x > 0);
  print(`${all.length} ${all.join(",")} ${none.length} ${fromEmpty.length}`);

  // A callback that throws at an element in the middle.
  const tried: i32[] = xs.map((x: i32): i32 => x + 10);
  try {
    const stopped: i32[] = tried.filter((x: i32): boolean => {
      if (x === 14) {
        throw new Error(`stop at ${x}`);
      }
      return true;
    });
    print(`not reached ${stopped.length}`);
  } catch (e) {
    if (e instanceof Error) {
      print(`caught ${e.message}`);
    }
  }

  // Not in the form: a local with a second read.
  const second: i32[] = xs.map((x: i32): i32 => x * 10);
  const fromSecond: i32[] = second.filter((x: i32): boolean => x > 40);
  print(`${fromSecond.join(",")} ${second.join(",")}`);

  // Not in the form: a local that a lambda captures.
  const captured: i32[] = xs.map((x: i32): i32 => x + 100);
  const fromCaptured: i32[] = captured.filter((x: i32): boolean => x > captured[5]);
  print(`${fromCaptured.join(",")} ${captured.length}`);

  // Not in the form: a declaration outside a loop with the call inside.
  const outside: i32[] = xs.map((x: i32): i32 => x);
  const lengths: i32[] = [];
  for (let round: i32 = 0; round < 3; round++) {
    const evenPositions: i32[] = outside.filter((x: i32, i: i32): boolean => i % 2 === 0);
    lengths.push(evenPositions.length);
  }
  print(lengths.join(","));

  // Not in the form: a let local, and a parameter.
  let mutable: i32[] = xs.map((x: i32): i32 => x - 1);
  const fromMutable: i32[] = mutable.filter(isEven);
  print(`${fromMutable.join(",")} ${mutable.join(",")}`);
  mutable = [];
  print(`${mutable.length} ${keepParameter(xs)}`);
}

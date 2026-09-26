// corpus: accept/a71-json-parse
// purpose: Covers typed JSON.parse success, data failures, and numeric edges.
// exercises: JSON, typed parse, direct result, try-catch, duplicate keys, numeric ranges
// questions: Q28, Q5, Q9
// tsc: accepts; js-comparable: no Q28: JavaScript JSON.parse has no static target type to validate.
class Config {
  name: string;
  count: i32;

  constructor(name: string, count: i32) {
    this.name = name;
    this.count = count;
  }
}

class FloatConfig {
  narrow: f32;
  wide: f64;

  constructor(narrow: f32, wide: f64) {
    this.narrow = narrow;
    this.wide = wide;
  }
}

function parsesAsConfig(text: string): boolean {
  try {
    const config: Config = JSON.parse(text);
    return config.count >= 0 || config.count < 0;
  } catch {
    return false;
  }
}

function parsesAsFloats(text: string): boolean {
  try {
    const config: FloatConfig = JSON.parse(text);
    return config.wide === config.wide || config.narrow !== config.narrow;
  } catch {
    return false;
  }
}

export function main(): void {
  const success: Config = JSON.parse<Config>('{"name":"demo","count":3}');
  print(`success-ok=${parsesAsConfig('{"name":"demo","count":3}')}`);
  print(`success=${success.name}:${success.count}`);

  print(`malformed-ok=${parsesAsConfig('{"name":"demo","count":')}`);
  print(`mismatch-ok=${parsesAsConfig('{"name":"demo","count":"three"}')}`);
  print(`missing-ok=${parsesAsConfig('{"name":"demo"}')}`);

  let arrayOk: boolean = true;
  try {
    const values: i32[] = JSON.parse<i32[]>('[1,2,"three"]');
    arrayOk = values.length >= 0;
  } catch {
    arrayOk = false;
  }
  print(`array-mismatch-ok=${arrayOk}`);

  const duplicate: Config =
    JSON.parse<Config>('{"name":"first","name":"last","count":4}');
  print(`duplicate=${duplicate.name}`);

  const negativeZero: f64 = JSON.parse<f64>("-0");
  print(`negative-zero-reciprocal=${1.0 / negativeZero}`);

  const beyondSafe: f64 = JSON.parse<f64>("9007199254740993");
  print(`beyond-safe=${beyondSafe}`);

  const beyondSafeInteger: i64 = JSON.parse<i64>("9007199254740993");
  print(`beyond-safe-i64=${beyondSafeInteger}`);

  let overflowOk: boolean = true;
  try {
    const overflow: f64 = JSON.parse<f64>("1e400");
    overflowOk = overflow === overflow;
  } catch {
    overflowOk = false;
  }
  print(`overflow-ok=${overflowOk}`);

  print(`narrow-overflow-ok=${parsesAsFloats('{"narrow":1e400,"wide":1}')}`);
  print(`wide-overflow-ok=${parsesAsFloats('{"narrow":1,"wide":1e400}')}`);
}

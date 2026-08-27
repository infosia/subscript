// corpus: accept/a71-json-parse
// purpose: Covers typed JSON.parse success, data failures, and numeric edges.
// exercises: JSON, typed parse, JsonResult, duplicate keys, numeric ranges
// questions: Q28, Q5, Q6
// tsc: accepts
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

export function main(): void {
  const success: JsonResult<Config> =
    JSON.parse<Config>('{"name":"demo","count":3}');
  print(`success-ok=${success.ok}`);
  if (success.ok) {
    print(`success=${success.value.name}:${success.value.count}`);
  }
  Context.free(success);

  const malformed: JsonResult<Config> =
    JSON.parse<Config>('{"name":"demo","count":');
  print(`malformed-ok=${malformed.ok}`);
  Context.free(malformed);

  const mismatch: JsonResult<Config> =
    JSON.parse<Config>('{"name":"demo","count":"three"}');
  print(`mismatch-ok=${mismatch.ok}`);
  Context.free(mismatch);

  const missing: JsonResult<Config> =
    JSON.parse<Config>('{"name":"demo"}');
  print(`missing-ok=${missing.ok}`);
  Context.free(missing);

  const arrayMismatch: JsonResult<i32[]> =
    JSON.parse<i32[]>('[1,2,"three"]');
  print(`array-mismatch-ok=${arrayMismatch.ok}`);
  Context.free(arrayMismatch);

  const duplicate: JsonResult<Config> =
    JSON.parse<Config>('{"name":"first","name":"last","count":4}');
  if (duplicate.ok) {
    print(`duplicate=${duplicate.value.name}`);
  }
  Context.free(duplicate);

  const negativeZero: JsonResult<f64> = JSON.parse<f64>("-0");
  if (negativeZero.ok) {
    print(`negative-zero-reciprocal=${1.0 / negativeZero.value}`);
  }
  Context.free(negativeZero);

  const beyondSafe: JsonResult<f64> = JSON.parse<f64>("9007199254740993");
  if (beyondSafe.ok) {
    print(`beyond-safe=${beyondSafe.value}`);
  }
  Context.free(beyondSafe);

  const beyondSafeInteger: JsonResult<i64> =
    JSON.parse<i64>("9007199254740993");
  if (beyondSafeInteger.ok) {
    print(`beyond-safe-i64=${beyondSafeInteger.value}`);
  }
  Context.free(beyondSafeInteger);

  const overflow: JsonResult<f64> = JSON.parse<f64>("1e400");
  print(`overflow-ok=${overflow.ok}`);
  Context.free(overflow);

  const narrowOverflow: JsonResult<FloatConfig> =
    JSON.parse<FloatConfig>('{"narrow":1e400,"wide":1}');
  print(`narrow-overflow-ok=${narrowOverflow.ok}`);
  Context.free(narrowOverflow);

  const wideOverflow: JsonResult<FloatConfig> =
    JSON.parse<FloatConfig>('{"narrow":1,"wide":1e400}');
  print(`wide-overflow-ok=${wideOverflow.ok}`);
  Context.free(wideOverflow);
}

// corpus: accept/a72-json-parse-limits
// purpose: Reports parse-side representation and input-depth failures as caught exceptions.
// exercises: JSON, typed parse, try-catch, depth limit, UTF-8, f32 range
// questions: Q28, Q5, Q9
// tsc: accepts; js-comparable: no Q5 Q28: JavaScript JSON.parse has no static target type to validate.
export function main(): void {
  let loneSurrogateOk: boolean = true;
  try {
    const loneSurrogate: string = JSON.parse<string>('"\\ud800"');
    loneSurrogateOk = loneSurrogate.length >= 0;
  } catch {
    loneSurrogateOk = false;
  }
  print(`lone-surrogate-ok=${loneSurrogateOk}`);

  let f32OverflowOk: boolean = true;
  try {
    const f32Overflow: f32 = JSON.parse<f32>("1e39");
    f32OverflowOk = f32Overflow === f32Overflow;
  } catch {
    f32OverflowOk = false;
  }
  print(`f32-overflow-ok=${f32OverflowOk}`);

  const tooDeepText: string =
    "[".repeat(129) + "0" + "]".repeat(129);
  let tooDeepOk: boolean = true;
  try {
    const tooDeep: i32 = JSON.parse<i32>(tooDeepText);
    tooDeepOk = tooDeep === tooDeep;
  } catch {
    tooDeepOk = false;
  }
  print(`depth-limit-ok=${tooDeepOk}`);
}

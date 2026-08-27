// corpus: accept/a72-json-parse-limits
// purpose: Reports parse-side representation and input-depth failures as data.
// exercises: JSON, typed parse, JsonResult, depth limit, UTF-8, f32 range
// questions: Q28, Q5, Q6
// tsc: accepts
export function main(): void {
  const loneSurrogate: JsonResult<string> =
    JSON.parse<string>('"\\ud800"');
  print(`lone-surrogate-ok=${loneSurrogate.ok}`);
  Context.free(loneSurrogate);

  const f32Overflow: JsonResult<f32> = JSON.parse<f32>("1e39");
  print(`f32-overflow-ok=${f32Overflow.ok}`);
  Context.free(f32Overflow);

  const tooDeepText: string =
    "[".repeat(129) + "0" + "]".repeat(129);
  const tooDeep: JsonResult<i32> = JSON.parse<i32>(tooDeepText);
  print(`depth-limit-ok=${tooDeep.ok}`);
  Context.free(tooDeep);
}

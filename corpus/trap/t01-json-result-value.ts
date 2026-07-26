// corpus: trap/t01-json-result-value
// purpose: Traps when a failed typed JSON result payload is read.
// exercises: JSON, typed parse, JsonResult, guarded payload
// questions: Q28
// expected-trap: json-result-value at the value member

export function main(): void {
  const failed: JsonResult<i32> = JSON.parse<i32>("nope");
  print(`${failed.value}`);
}

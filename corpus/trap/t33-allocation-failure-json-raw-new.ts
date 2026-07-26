// corpus: trap/t33-allocation-failure-json-raw-new
// purpose: Injects allocation failure at checker-generated JSON `RawNew`.
// exercises: allocation-failure, JSON.parse, RawNew
// questions: none
// tier-policy: both tiers must report the same trap tuple and pre-fault stdout at the same object-allocation count

class Config {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }
}

export function main(): void {
  print("before");
  const result: JsonResult<Config> =
    JSON.parse<Config>("{\"value\":7}");
  if (result.ok) {
    print(`${result.value.value}`);
  }
}

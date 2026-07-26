// corpus: reject/r60-json-parse-no-context
// purpose: Rejects JSON.parse without a static target type.
// exercises: JSON, typed parse, contextual typing
// questions: Q28
// expected-error: S014 at the parse member

export function main(): void {
  JSON.parse("{}");
}

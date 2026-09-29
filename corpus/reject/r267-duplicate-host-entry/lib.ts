// corpus: reject/r267-duplicate-host-entry/lib
// purpose: Rejects a second host entry name in module order (C14).
// exercises: module-export, host-entry
// questions: compiler.md §125, C14
// tsc: accepts
// expected-error: S017 at line 8, the second host entry declaration (C14)

export function update(): void {}

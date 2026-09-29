// corpus: reject/r267-duplicate-host-entry/main
// purpose: Loads a second module with the same host entry name.
// exercises: module-import, host-entry
// questions: compiler.md §125, C14
// collision: C14

import "./lib";
export function update(): void {}
export function main(): void { update(); }

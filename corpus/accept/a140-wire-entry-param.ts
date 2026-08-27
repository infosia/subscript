// corpus: accept/a140-wire-entry-param
// purpose: Proves that a host passes one wire-mapped alias and one scalar to a script entry.
// exercises: host-callable-export-parameters, CEnum, unknown-wire-value-validation
// questions: R32
// tsc: accepts
let configuredMode: SubWireMode = "m0";
let configuredTag: i32 = 0;

export function configure(mode: SubWireMode, tag: i32): void {
  configuredMode = mode;
  configuredTag = tag;
}

export function main(): void {
  print(`wire-entry:mode=${configuredMode}`);
  print(`wire-entry:tag=${configuredTag}`);
}

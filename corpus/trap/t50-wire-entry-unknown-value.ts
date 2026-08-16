// corpus: trap/t50-wire-entry-unknown-value
// purpose: An unknown host wire value traps before a script entry runs.
// exercises: host-callable-export-parameters, CEnum, unknown-wire-value, trap-stop
// questions: R32, C6
// tier-policy: both tiers trap with kind 24
// expected-trap: wire-enum-unknown-value for SubWireMode value 12345 at the parameter declaration

export function configure(mode: SubWireMode, tag: i32): void {
  print(`wire-entry:mode=${mode}`);
  print(`wire-entry:tag=${tag}`);
}

export function main(): void {}

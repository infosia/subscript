// corpus: trap/t47-unreachable-reached
// purpose: Reaching unreachable() traps at its call site under C6 trap-stop semantics.
// exercises: unreachable, divergence-flow, trap-stop
// questions: R15, C6
// tier-policy: both tiers trap with kind 23
// expected-trap: unreachable-reached at the unreachable() call

export function main(): void {
  print("before");
  unreachable();
  print("after");
}

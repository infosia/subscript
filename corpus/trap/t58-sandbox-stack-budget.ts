// corpus: trap/t58-sandbox-stack-budget
// profile: sandbox
// purpose: Passes the 512 KiB default stack budget of the sandbox profile.
// exercises: sandbox-profile, stack-budget, trap-stop
// questions: Q7, compiler section 109
// tier-policy: every tier traps with kind 27 at the entry of the recursive function; the depth reached is tier-specific, so the entry prints nothing that depends on it
// expected-trap: stack-budget at the entry of the recursive function `descend`; the Sandbox.Enter checkpoint carries the callee's position, not the call site's
function descend(depth: i32): i32 {
  return descend(depth + 1) + 1;
}

export function main(): void {
  print("start");
  print(`${descend(0)}`);
}

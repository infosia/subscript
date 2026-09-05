// corpus: accept/a137-handle-entry-param
// interpreter: no — exported main requires a host-supplied handle
// purpose: Proves that a host passes one opaque handle and one scalar to a script entry.
// exercises: host-callable-export-parameters, borrowed-opaque-handle, stored-wrapper
// questions: R30
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
class AdoptedState {
  state: SubHostOwnedState;

  constructor(state: SubHostOwnedState) {
    this.state = state;
  }

  advance(): i32 {
    return subHostOwnedStateAdvance(this.state);
  }
}

let adopted: AdoptedState | null = null;

export function adopt(state: SubHostOwnedState, tag: i32): void {
  adopted = new AdoptedState(state);
  print(`handle-entry:tag=${tag}`);
}

export function main(): void {
  if (adopted === null) {
    print("handle-entry:missing");
    return;
  }
  print(`handle-entry:first=${adopted.advance()}`);
  print(`handle-entry:second=${adopted.advance()}`);
}

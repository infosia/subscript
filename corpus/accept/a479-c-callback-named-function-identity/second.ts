// corpus: accept/a479-c-callback-named-function-identity/second
// purpose: Converts the shared function in another module.
// exercises: C-callback, named-function, module-export
// questions: compiler.md §328 rule 2
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
import { onLog, Cookie } from "./handlers";
export function second(ud: Cookie): SubCallbackInfo {
  return new SubCallbackInfo(onLog, ud, null);
}

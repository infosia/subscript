// corpus: accept/a479-c-callback-named-function-identity/first
// purpose: Converts the shared function in one module.
// exercises: C-callback, named-function, module-export
// questions: compiler.md §328 rule 2
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
import { onLog, Cookie } from "./handlers";
export function first(ud: Cookie): SubCallbackInfo {
  return new SubCallbackInfo(onLog, ud, null);
}

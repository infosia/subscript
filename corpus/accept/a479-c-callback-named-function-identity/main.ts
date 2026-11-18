// corpus: accept/a479-c-callback-named-function-identity/main
// interpreter: no — calls the synthetic native interop library
// purpose: A named function keeps its C callback identity across sites and modules.
// exercises: C-callback, named-function, module-import, callback-identity
// questions: compiler.md §328 rules 1 and 2
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
import { onLog, otherLog, onIdle, Cookie } from "./handlers";
import { first } from "./first";
import { second } from "./second";

export function main(): void {
  const ud: Cookie = new Cookie();
  print(`${sub328SameCallbacks(first(ud), second(ud))}`);
  print(`${sub328SameCallbacks(first(ud), new SubCallbackInfo(onLog, ud, null))}`);
  print(`${sub328SameCallbacks(first(ud), new SubCallbackInfo(otherLog, ud, null))}`);
  print(`${sub328SameCallbacks(first(ud), first(new Cookie()))}`);
  print(`${sub328SameCallbacks(new SubCallbackInfo(onIdle, null, null), new SubCallbackInfo(onIdle, null, null))}`);
}

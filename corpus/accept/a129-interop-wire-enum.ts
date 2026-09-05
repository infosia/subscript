// corpus: accept/a129-interop-wire-enum
// interpreter: no — calls the synthetic native interop library
// purpose: Crosses a synthetic C boundary through an R23 wire-mapped literal union.
// exercises: CEnum, foreign-return, foreign-parameter, switch, non-dense-wire-values
// questions: R23, Q32
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
export function main(): void {
  const received: SubWireMode = subWireModeNext();
  switch (received) {
    case "m0":
      print(`received=m0:${received}`);
      break;
    case "m1":
      print(`received=m1:${received}`);
      break;
    case "m2":
      print(`received=m2:${received}`);
      break;
  }

  print(`m0=${subWireModeEcho("m0")}`);
  print(`m1=${subWireModeEcho("m1")}`);
  print(`m2=${subWireModeEcho("m2")}`);
}

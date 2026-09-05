// corpus: accept/a130-interop-wire-enum-bind
// interpreter: no — calls the synthetic native interop library
// purpose: Crosses C through an enum typedef mapped by bind to an ambient CEnum alias.
// exercises: subscript-cenum, enum-typedef, foreign-return, foreign-parameter, switch
// questions: R24, R23, Q32
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
export function main(): void {
  const received: SubBindTone = subBindToneNext();
  switch (received) {
    case "quiet":
      print(`received=quiet:${received}`);
      break;
    case "steady":
      print(`received=steady:${received}`);
      break;
    case "bright":
      print(`received=bright:${received}`);
      break;
  }

  print(`bright=${subBindToneEcho("bright")}`);
}

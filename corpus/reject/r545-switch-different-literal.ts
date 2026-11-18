// corpus: reject/r545-switch-different-literal
// purpose: A case label cannot compare a different literal value.
// exercises: switch, const, literal
// questions: compiler.md §330 rule 3
// tsc: rejects TS2678
// expected-error: S100 at line 12
const MODE = 1;
export function main(): void {
  switch (MODE) {
    case 1:
      break;
    case 2:
      break;
  }
}

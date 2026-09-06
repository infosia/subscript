// corpus: reject/r185-enum-member-at-eof
// purpose: Reject an enum member at EOF without a panic or fault.
// exercises: enums, syntax-errors, eof
// questions: none
// tsc: rejects TS1005
// expected-error: S100 at the enum member

enum Status {
  Ready
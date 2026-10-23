// A program that TypeScript accepts and subscript does not. `tsc --strict`
// accepts this file. subscript checks the whole program before it runs,
// and it stops at the type `number`. The program does not start:
//
//   $ subscript run examples/typescript/rejected.ts
//   error[S007]: bare `number` is rejected; there is no default numeric type — use a sized type (i8, u8, i16, u16, i32, u32, i64, u64, f16, f32, f64)
//    --> examples/typescript/rejected.ts:27:16
//      |
//   27 |   const count: number = 3;
//      |                ^
//      = rule: Bare `number` is rejected; sized numeric types are mandatory.
//      = TypeScript accepts:
//      |   const count: number = 3;
//      = subscript:
//      |   const count: i32 = 3;
//      = why: `number` is a 64-bit float with no C width, so every declaration names one of the sized types. (collisions.md C3)
//   error: 1 error(s)
//
// S007 is the code of the rule. Each code keeps its meaning, so you can
// look it up. The line `--> ...` gives the file, the line, and the
// column. The `TypeScript accepts` and `subscript` lines show the form
// that fails and the form to write in its place.
//
// To fix this program, write a sized type: `const count: i32 = 3;`.

export function main(): void {
  const count: number = 3;
  print(`${count} apples`);
}

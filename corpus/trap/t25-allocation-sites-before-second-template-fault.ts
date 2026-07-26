// corpus: trap/t25-allocation-sites-before-second-template-fault
// purpose: Traverses every listed allocation-bearing expression before a later fault.
// exercises: string-literal, str-concat, fmt-all, array-literal, template-multiplicity
// questions: none
// expected-trap: division-by-zero in the second interpolation

export function main(): void {
  const i: i32 = -7;
  const u: u32 = 7;
  const il: i64 = -8;
  const ul: u64 = 8;
  const f: f32 = 1.5;
  const d: f64 = 2.5;
  const b: boolean = true;
  const formatted: string = `${i},${u},${il},${ul},${f},${d},${b}`;
  const joined: string = "prefix:" + formatted;
  const values: string[] = [joined, "tail"];
  print(values[0]);

  const zero: i32 = 0;
  const fault: string = `first ${i}; second ${84 / zero}`;
  print(fault);
}

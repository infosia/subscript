// corpus: accept/a126-interop-by-value-packing
// purpose: Pins target-ABI register images for by-value boundary structs, including AAPCS64 eightbyte packing, HFAs, padding, and indirect composites.
// exercises: interop-by-value, aapcs64-eightbytes, hfa, c-layout-padding, indirect-composite
// questions: OBS-4
// tsc: accepts
export function main(): void {
  const i32One: SubByValueI32One = new SubByValueI32One(3);
  const i32OneReport: SubByValueI32One = new SubByValueI32One(0);
  subByValueI32OneReport(i32OneReport, i32One);
  print(`{i32} a=${i32OneReport.a}`);

  const i32Pair: SubByValueI32Pair = new SubByValueI32Pair(3, 7);
  const i32PairReport: SubByValueI32Pair = new SubByValueI32Pair(0, 0);
  subByValueI32PairReport(i32PairReport, i32Pair);
  print(`{i32,i32} x=${i32PairReport.x} y=${i32PairReport.y}`);

  const i32Triple: SubByValueI32Triple = new SubByValueI32Triple(3, 7, 11);
  const i32TripleReport: SubByValueI32Triple = new SubByValueI32Triple(0, 0, 0);
  subByValueI32TripleReport(i32TripleReport, i32Triple);
  print(`{i32,i32,i32} a=${i32TripleReport.a} b=${i32TripleReport.b} c=${i32TripleReport.c}`);

  const narrow: SubByValueI16I16I32 = new SubByValueI16I16I32(-3, 7, 11);
  const narrowReport: SubByValueI16I16I32 = new SubByValueI16I16I32(0, 0, 0);
  subByValueI16I16I32Report(narrowReport, narrow);
  print(`{i16,i16,i32} a=${narrowReport.a} b=${narrowReport.b} c=${narrowReport.c}`);

  const bytes: SubByValueU8Four = new SubByValueU8Four(3, 7, 11, 13);
  const bytesReport: SubByValueU8Four = new SubByValueU8Four(0, 0, 0, 0);
  subByValueU8FourReport(bytesReport, bytes);
  print(`{u8,u8,u8,u8} a=${bytesReport.a} b=${bytesReport.b} c=${bytesReport.c} d=${bytesReport.d}`);

  const i64Pair: SubByValueI64Pair = new SubByValueI64Pair(3, 7);
  const i64PairReport: SubByValueI64Pair = new SubByValueI64Pair(0, 0);
  subByValueI64PairReport(i64PairReport, i64Pair);
  print(`{i64,i64} a=${i64PairReport.a} b=${i64PairReport.b}`);

  const hfa2: SubByValueF32Hfa2 = new SubByValueF32Hfa2(3.25, 7.5);
  const hfa2Report: SubByValueF32Hfa2 = new SubByValueF32Hfa2(0.0, 0.0);
  subByValueF32Hfa2Report(hfa2Report, hfa2);
  print(`{f32,f32} a=${hfa2Report.a} b=${hfa2Report.b}`);

  const hfa4: SubByValueF32Hfa4 = new SubByValueF32Hfa4(1.25, 2.5, 3.75, 4.5);
  const hfa4Report: SubByValueF32Hfa4 = new SubByValueF32Hfa4(0.0, 0.0, 0.0, 0.0);
  subByValueF32Hfa4Report(hfa4Report, hfa4);
  print(`{f32,f32,f32,f32} a=${hfa4Report.a} b=${hfa4Report.b} c=${hfa4Report.c} d=${hfa4Report.d}`);

  const mixed: SubByValueI32F32 = new SubByValueI32F32(3, 7.5);
  const mixedReport: SubByValueI32F32 = new SubByValueI32F32(0, 0.0);
  subByValueI32F32Report(mixedReport, mixed);
  print(`{i32,f32} a=${mixedReport.a} b=${mixedReport.b}`);

  const padded: SubByValueI32I64 = new SubByValueI32I64(3, 7);
  const paddedReport: SubByValueI32I64 = new SubByValueI32I64(0, 0);
  subByValueI32I64Report(paddedReport, padded);
  print(`{i32,pad,i64} a=${paddedReport.a} b=${paddedReport.b}`);

  const indirect: SubByValueI64Triple = new SubByValueI64Triple(3, 7, 11);
  const indirectReport: SubByValueI64Triple = new SubByValueI64Triple(0, 0, 0);
  subByValueI64TripleReport(indirectReport, indirect);
  print(`{i64,i64,i64} a=${indirectReport.a} b=${indirectReport.b} c=${indirectReport.c}`);
}

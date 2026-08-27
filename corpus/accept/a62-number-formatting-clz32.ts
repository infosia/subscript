// corpus: accept/a62-number-formatting-clz32
// purpose: Pins Q26 radix and precision formatting plus zero-defined clz32.
// exercises: number-to-string-radix, number-to-exponential,
//            number-to-precision, math-clz32
// questions: Q19, Q26
// tsc: accepts; js-comparable: yes
export function main(): void {
  const integer: f64 = 255.0;
  const fraction: f64 = 1234.5678;
  const single: f32 = 10.5;

  print(`radix-integral ${integer.toString(2)} ${integer.toString(8)} ${integer.toString(16)} ${(123456.0).toString(36)}`);
  print(`radix-fraction ${(0.5).toString(2)} ${(10.125).toString(8)} ${(123.456).toString(16)} ${fraction.toString(36)}`);
  print(`radix-negative ${(-255.0).toString(16)} ${(-255.0).toString(2)}`);
  print(`radix-special ${Number.NaN.toString(16)} ${Number.POSITIVE_INFINITY.toString(2)} ${Number.NEGATIVE_INFINITY.toString(36)}`);
  print(`radix-decimal ${fraction.toString(10)} ${fraction}`);
  print(`radix-f32 ${single.toString(2)} ${single.toString(10)}`);

  print(`exponential ${123456.0.toExponential()} ${123456.0.toExponential(2)} ${(0.999).toExponential(0)} ${(0.0).toExponential(2)}`);
  print(`exponential-special ${Number.NaN.toExponential()} ${Number.POSITIVE_INFINITY.toExponential(2)} ${Number.NEGATIVE_INFINITY.toExponential()}`);

  print(`precision ${123.456.toPrecision(2)} ${0.000123.toPrecision(2)} ${0.000000123.toPrecision(2)} ${9.99.toPrecision(2)}`);
  print(`precision-special ${Number.NaN.toPrecision(2)} ${Number.POSITIVE_INFINITY.toPrecision(2)} ${Number.NEGATIVE_INFINITY.toPrecision(2)}`);

  print(`clz32 ${Math.clz32(0)} ${Math.clz32(1)} ${Math.clz32(2147483648)} ${Math.clz32(4294967295)}`);
}

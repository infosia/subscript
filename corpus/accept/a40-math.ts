// corpus: accept/a40-math
// purpose: Exercises the accepted Math subset: every function and
//          constant of stdlib.md §1, plus the pinned ECMA edge
//          semantics (round/sign/max/min/pow/abs on NaN and signed
//          zero) formatted per Q14.
// exercises: math-intrinsics, math-constants, q14-formatting
// questions: Q14, Q19
// tsc: accepts
export function main(): void {
  // Constants (folded to f64 literals at check time).
  print(`E ${Math.E}`);
  print(`LN2 ${Math.LN2}`);
  print(`LN10 ${Math.LN10}`);
  print(`LOG2E ${Math.LOG2E}`);
  print(`LOG10E ${Math.LOG10E}`);
  print(`PI ${Math.PI}`);
  print(`SQRT1_2 ${Math.SQRT1_2}`);
  print(`SQRT2 ${Math.SQRT2}`);
  // Unary functions.
  print(`abs ${Math.abs(-3.5)}`);
  print(`acos ${Math.acos(1)}`);
  print(`acosh ${Math.acosh(1)}`);
  print(`asin ${Math.asin(0)}`);
  print(`asinh ${Math.asinh(0)}`);
  print(`atan ${Math.atan(0)}`);
  print(`atanh ${Math.atanh(0)}`);
  print(`cbrt ${Math.cbrt(27)}`);
  print(`ceil ${Math.ceil(1.2)}`);
  print(`cos ${Math.cos(0)}`);
  print(`cosh ${Math.cosh(0)}`);
  print(`exp ${Math.exp(0)}`);
  print(`expm1 ${Math.expm1(0)}`);
  print(`floor ${Math.floor(1.8)}`);
  print(`log ${Math.log(1)}`);
  print(`log1p ${Math.log1p(0)}`);
  print(`log10 ${Math.log10(1000)}`);
  print(`log2 ${Math.log2(8)}`);
  print(`round ${Math.round(2.4)}`);
  print(`sign ${Math.sign(-7.5)}`);
  print(`sin ${Math.sin(0)}`);
  print(`sinh ${Math.sinh(0)}`);
  print(`sqrt ${Math.sqrt(9)}`);
  print(`tan ${Math.tan(0)}`);
  print(`tanh ${Math.tanh(0)}`);
  print(`trunc ${Math.trunc(-1.7)}`);
  // Binary functions.
  print(`atan2 ${Math.atan2(1, 1)}`);
  print(`hypot ${Math.hypot(3, 4)}`);
  print(`pow ${Math.pow(2, 10)}`);
  print(`max ${Math.max(2.5, 7)}`);
  print(`min ${Math.min(2.5, 7)}`);
  // ECMA edge battery (stdlib.md §1), pinned by this golden.
  const nan: f64 = Math.sqrt(-1);
  const inf: f64 = 1.0 / 0.0;
  print(`round(-2.5) ${Math.round(-2.5)}`);
  print(`round(2.5) ${Math.round(2.5)}`);
  print(`round(-0.4) ${Math.round(-0.4)}`);
  print(`sign(-0) ${Math.sign(-0)}`);
  print(`sign(0) ${Math.sign(0)}`);
  print(`max(NaN,1) ${Math.max(nan, 1)}`);
  print(`min(1,NaN) ${Math.min(1, nan)}`);
  print(`max(0,-0) ${Math.max(0, -0)}`);
  print(`min(0,-0) ${Math.min(0, -0)}`);
  print(`pow(NaN,0) ${Math.pow(nan, 0)}`);
  print(`pow(1,Inf) ${Math.pow(1, inf)}`);
  print(`pow(1,NaN) ${Math.pow(1, nan)}`);
  print(`abs(-0) ${Math.abs(-0)}`);
  print(`sqrt(-1) ${Math.sqrt(-1)}`);
}

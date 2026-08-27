// corpus: accept/a58-number-parse
// purpose: Exercises required-radix parseInt and ECMA longest-prefix
//          parseInt/parseFloat, including NaN as parse failure data.
// exercises: parse-int, parse-float, number-is-nan, numeric-cast
// questions: Q25
// tsc: accepts; js-comparable: yes
export function main(): void {
  print(`radix ${parseInt("101tail", 2)} ${parseInt("-0xFz", 16)} ${parseInt("z!", 36)}`);
  print(`space-sign ${parseInt(" \t +42done", 10)} ${parseInt("\uFEFF-11", 10)}`);
  print(`large ${parseInt("900719925474099267rest", 10)}`);
  print(`int-fail ${Number.isNaN(parseInt("2", 2))} ${Number.isNaN(parseInt("xyz", 10))}`);

  print(`float ${parseFloat("1.5abc")} ${parseFloat("  -1.25e2tail")} ${parseFloat(".5x")}`);
  print(`float-prefix ${parseFloat("1e+")} ${parseFloat("Infinity!")} ${parseFloat("-Infinity?")}`);
  print(`float-fail ${Number.isNaN(parseFloat("not-a-number"))}`);

  const parsed: f64 = parseInt("7f", 16);
  const exact: i32 = parsed as i32;
  print(`cast ${exact}`);
}

// corpus: accept/a43-string
// purpose: Exercises the accepted String method subset of stdlib.md §8
//          (Q21: byte measures, Unicode case mapping and ECMA whitespace):
//          indexOf/lastIndexOf (incl. empty needle and from-clamping),
//          includes/startsWith/endsWith, charCodeAt byte values, split
//          (multi, no-match, adjacent, leading/trailing separators), the
//          trim family, repeat, padStart/padEnd (incl. multi-byte pad
//          truncation), toUpperCase/toLowerCase round-trip, and literal
//          replace/replaceAll (including Q27 `$` substitution).
// exercises: string-methods, q14-formatting
// questions: Q14, Q21
// tsc: accepts
export function main(): void {
  const s: string = "hello world";
  // indexOf: hit, miss, from, and the clamps (negative -> 0, beyond
  // length -> length); the empty needle returns the clamped from.
  print(`iof ${s.indexOf("o")}`);
  print(`iofmiss ${s.indexOf("z")}`);
  print(`ioffrom ${s.indexOf("o", 5)}`);
  print(`iofneg ${s.indexOf("o", -3)}`);
  print(`iofbeyond ${s.indexOf("o", 99)}`);
  print(`iofempty ${s.indexOf("")}`);
  print(`iofemptyfrom ${s.indexOf("", 99)}`);
  // lastIndexOf; the empty needle returns the length.
  print(`liof ${s.lastIndexOf("o")}`);
  print(`liofmiss ${s.lastIndexOf("z")}`);
  print(`liofempty ${s.lastIndexOf("")}`);
  // includes / startsWith / endsWith; the empty needle is included.
  const inc: boolean = s.includes("lo w");
  print(`inc ${inc}`);
  print(`incfrom ${s.includes("hello", 1)}`);
  print(`incempty ${s.includes("")}`);
  print(`starts ${s.startsWith("hell")}`);
  print(`startsnot ${s.startsWith("world")}`);
  print(`ends ${s.endsWith("world")}`);
  print(`endsnot ${s.endsWith("hello")}`);
  // charCodeAt: the byte value (Q21).
  print(`cca ${"ABC".charCodeAt(0)}`);
  print(`ccalast ${s.charCodeAt(10)}`);
  // split: multi with adjacent separators, no-match -> [whole], and
  // leading/trailing separators produce empty strings.
  const parts: string[] = "a,b,,c".split(",");
  print(`split ${parts.length} ${parts[0]} ${parts[1]} [${parts[2]}] ${parts[3]}`);
  const whole: string[] = "ab".split("x");
  print(`splitnone ${whole.length} ${whole[0]}`);
  const edged: string[] = ",a,".split(",");
  print(`splitedge ${edged.length} [${edged[0]}] ${edged[1]} [${edged[2]}]`);
  // trim family: the ASCII members of ECMA whitespace.
  const padded: string = "  x\t";
  print(`trim [${padded.trim()}]`);
  print(`trimstart [${padded.trimStart()}]`);
  print(`trimend [${padded.trimEnd()}]`);
  print(`trimall [${" \t\n\r\f\v ".trim()}]`);
  // repeat: 0 -> "", 1, 3.
  print(`rep0 [${"ab".repeat(0)}]`);
  print(`rep1 ${"ab".repeat(1)}`);
  print(`rep3 ${"ab".repeat(3)}`);
  // padStart/padEnd: default space pad, exact and already-long-enough
  // receivers unchanged, and the final pad repeat truncated to the
  // target length ("ab" to 5 with "xy" -> "xyxab" / "abxyx").
  print(`pads [${"7".padStart(3)}]`);
  print(`pade [${"7".padEnd(3)}]`);
  print(`padexact ${"abc".padStart(3, "x")}`);
  print(`padlong ${"abcd".padStart(2, "x")}`);
  print(`padtrunc ${"ab".padStart(5, "xy")}`);
  print(`padtrunce ${"ab".padEnd(5, "xy")}`);
  // ASCII cases within Unicode Default Case Conversion.
  print(`up ${"mix 3d!".toUpperCase()}`);
  print(`low ${"MIX 3D!".toLowerCase()}`);
  // replace: first occurrence only; replaceAll: every occurrence, one
  // left-to-right pass (a replacement is never rescanned).
  print(`rep ${"aaa".replace("a", "b")}`);
  print(`repall ${"abcabc".replaceAll("bc", "X")}`);
  print(`repgrow ${"aa".replaceAll("a", "aa")}`);
  print(`repmiss ${"abc".replace("z", "y")}`);
  // Q27 closes the old Q21 divergence: `$&` expands to the match.
  print(`repdollar ${"x=1".replace("1", "$&")}`);
}

// corpus: accept/a83-regex-review
// questions: Q31
// purpose: Pins regex source rendering inside character classes, every
//          accepted flag, literal reuse across collection, and the
//          documented non-BMP matching divergence with and without u.
// expected: corpus/accept/a83-regex-review.expected

export function main(): void {
  print(`source ${/[/]/.source} ${/a[/]b/.source} ${new RegExp("[/]").source} ${new RegExp("/").source}`);
  print(`flags ${/a/d.flags} ${/a/g.flags} ${/a/i.flags} ${/a/m.flags} ${/a/s.flags} ${/a/u.flags} ${new RegExp("a", "v").flags}`);

  const scalarPieces: string[] = "😀".split(/(?:)/);
  const unicodePieces: string[] = "😀".split(/(?:)/u);
  print(`nonBmpSplit ${scalarPieces.length}:${scalarPieces.join("|")} ${unicodePieces.length}:${unicodePieces.join("|")}`);
  print(`nonBmpDot ${"😀".replaceAll(/./g, "X")} ${"😀".replaceAll(/./gu, "X")}`);

  let matches: i32 = 0;
  for (let i: i32 = 0; i < 3; i += 1) {
    if (/a/.test("a")) {
      matches += 1;
    }
    Context.collect();
  }
  print(`literalLoop ${matches}`);
}

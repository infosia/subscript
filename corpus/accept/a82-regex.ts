// corpus: accept/a82-regex
// feature: regex
// purpose: Pins P23's cached, budgeted RegExp surface, whole-subject context
//          across repeated matching, UTF-8 byte offsets, capture extents,
//          capture-reinjecting split, empty-match progress, and every ECMA
//          replacement substitution form.
// expected: corpus/accept/a82-regex.expected

export function main(): void {
  const literal: RegExp = /(?<word>[a-z]+)-(\d+)/gi;
  const dynamic: RegExp = new RegExp("x", "mi");
  print(`meta ${literal.source} ${literal.flags} ${dynamic.source} ${dynamic.flags}`);
  print(`source ${new RegExp("").source} ${new RegExp("/").source}`);

  const captures: RegExp = /é(x)(?<digits>\d+)/;
  print(`capture ${captures.test("zzéx12")} ${captures.matchStart(0)} ${captures.matchEnd(0)} ${captures.matchStart(1)} ${captures.matchEnd(1)} ${captures.matchStart(2)} ${captures.matchEnd(2)}`);
  print(`search ${"éx12".search(/\d+/)}`);

  const substituted: string = "xabz".replace(
    /(?<word>a)(b)?/,
    "[$$][$&][$`][$'][$1][$2][$01][$10][$99][$<word>][$<missing>]",
  );
  print(`replace ${substituted}`);
  print(`replaceGlobal ${"a1 b22".replace(/\d+/g, "#")}`);
  print(`all ${"a1 b22".replaceAll(/(?<d>\d+)/g, "<$&:$1:$<d>>")}`);
  print(`empty ${"éx".replaceAll(/()/g, "-")}`);

  const pieces: string[] = "a1b22c".split(/(\d+)/);
  print(`split ${pieces.join("|")}`);
  const emptyPieces: string[] = "éx".split(/(?:)/);
  print(`splitEmpty ${emptyPieces.join("|")}`);

  print(`lookbehindReplace ${"XXX".replace(/(?<=X)X/, "Z")}`);
  print(`lookbehindAll ${"XXX".replaceAll(/(?<=X)X/g, "Z")}`);
  const lookbehindPieces: string[] = "XXX".split(/(?<=X)X/);
  print(`lookbehindSplit ${lookbehindPieces.length} ${lookbehindPieces.join("|")}`);

  print(`anchorReplace ${"XXX".replace(/^X/, "Z")}`);
  print(`anchorAll ${"XXX".replaceAll(/^X/g, "Z")}`);
  const anchorPieces: string[] = "XXX".split(/^X/);
  print(`anchorSplit ${anchorPieces.length} ${anchorPieces.join("|")}`);

  const lines: string = "X\nX\nX";
  print(`anchorMReplace ${lines.replace(/^X/m, "Z").replaceAll("\n", "~")}`);
  print(`anchorMAll ${lines.replaceAll(/^X/gm, "Z").replaceAll("\n", "~")}`);
  const linePieces: string[] = lines.split(/^X/m);
  print(`anchorMSplit ${linePieces.length} ${linePieces.join("|").replaceAll("\n", "~")}`);

  print(`spanningReplace ${"abc".replace(/ab|(?<=ab)c/, "Z")}`);
  print(`spanningAll ${"abc".replaceAll(/ab|(?<=ab)c/g, "Z")}`);
  const spanningPieces: string[] = "abc".split(/ab|(?<=ab)c/);
  print(`spanningSplit ${spanningPieces.length} ${spanningPieces.join("|")}`);

  print(`boundaryReplace ${"ab cd".replace(/\b[a-z]/, "Z")}`);
  print(`boundaryAll ${"ab cd".replaceAll(/\b[a-z]/g, "Z")}`);
  const boundaryPieces: string[] = "ab cd".split(/\b/);
  print(`boundarySplit ${boundaryPieces.length} ${boundaryPieces.join("|")}`);

  print(`emptyContextReplace ${"XX".replace(/(?<=X)/, "-")}`);
  print(`emptyContextAll ${"XX".replaceAll(/(?<=X)/g, "-")}`);
  const emptyContextPieces: string[] = "XX".split(/(?<=X)/);
  print(`emptyContextSplit ${emptyContextPieces.length} ${emptyContextPieces.join("|")}`);

  const unicodeOffsets: RegExp = /X/g;
  print(`unicodeAll ${"éXX".replaceAll(unicodeOffsets, "Z")}`);
  print(`unicodeOffsets ${unicodeOffsets.matchStart(0)} ${unicodeOffsets.matchEnd(0)}`);
  const unicodePieces: string[] = "éXX".split(/X/);
  print(`unicodeSplit ${unicodePieces.length} ${unicodePieces.join("|")}`);
}

// corpus: accept/a476-fixed-struct-array-members
// interpreter: no — calls the synthetic native interop library
// purpose: Fixed struct array members use each element pass (compiler.md §326).
// exercises: fixed-array-member, scratch-array, pair-receive, string-view, const-pointer
// questions: Q13
// tsc: accepts
// js-comparable: no Q13: The C boundary has no JavaScript shim.

export function main(): void {
  const pairs = new Sub326Pairs(3, [new SubDescReadLay(1, [7]), new SubDescReadLay(2, [8])]);
  print(`${sub326Read(pairs)} ${pairs.qs[0].tag} ${pairs.qs[1].tag}`);
  sub326Pairs(pairs);
  Context.collect();
  print(`${pairs.qs[0].tag} ${pairs.qs[0].items.length} ${pairs.qs[0].items[0]} ${pairs.qs[0].items[2]}`);
  print(`${pairs.qs[1].tag} ${pairs.qs[1].items.length} ${pairs.qs[1].items[0]} ${pairs.qs[1].items[1]}`);
  const strings = new Sub326Strings(4, [new SubDescReadStr("a", 1), new SubDescReadStr("b", 2)]);
  sub326Strings(strings);
  Context.collect();
  print(`${strings.ss[0].label} ${strings.ss[1].label} ${strings.ss[0].n} ${strings.ss[1].n}`);
  const bytes = new Sub326Bytes(5, [new SubByValueI32One(1), new SubByValueI32One(2)], "before");
  sub326Bytes(bytes);
  print(`${bytes.ps[0].a} ${bytes.ps[1].a} ${bytes.name}`);
}

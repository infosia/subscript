// corpus: accept/a265-regexp-flag-accessors
// purpose: RegExp flag accessors and source rendering (stdlib.md §15.3a).
// exercises: regexp
// questions: Q31
// tsc: accepts; js-comparable: yes
function show(re: RegExp): void {
  print(`${re.toString()} ${re.global} ${re.ignoreCase} ${re.multiline} ${re.dotAll} ${re.unicode} ${re.sticky} ${re.hasIndices} ${re.source} ${re.flags}`);
}
function dynamic(flags: string): void {
  show(new RegExp("a", flags));
}
export function main(): void {
  show(/a/);
  show(/a/gimsu);
  show(/a/d);
  show(new RegExp("a/b"));
  show(new RegExp(""));
  show(/[/]/);
  show(/a\/b/);
  dynamic("usmigd");
}

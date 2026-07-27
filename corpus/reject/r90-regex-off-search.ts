// corpus: reject/r90-regex-off-search
// feature: regex-off
// purpose: Regex-backed String.search is unavailable without P23.
// expected-error: S014 naming the missing Cargo feature

export function main(): void {
  print(`${"éx".search(/x/)}`);
}

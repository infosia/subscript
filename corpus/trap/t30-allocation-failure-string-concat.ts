// corpus: trap/t30-allocation-failure-string-concat
// purpose: Injects allocation failure at string concatenation.
// exercises: allocation-failure, string-concatenation
// questions: none
// tier-policy: both tiers must report the same trap tuple and pre-fault stdout at the same object-allocation count

export function main(): void {
  const left: string = "left";
  const right: string = "right";
  print("before");
  const joined: string = left + right;
  print(joined);
}

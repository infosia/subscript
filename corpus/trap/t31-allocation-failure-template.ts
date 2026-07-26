// corpus: trap/t31-allocation-failure-template
// purpose: Injects allocation failure while building a template.
// exercises: allocation-failure, template
// questions: none
// tier-policy: both tiers must report the same trap tuple and pre-fault stdout at the same object-allocation count

export function main(): void {
  const piece: string = "middle";
  print("before");
  const joined: string = `left${piece}right`;
  print(joined);
}

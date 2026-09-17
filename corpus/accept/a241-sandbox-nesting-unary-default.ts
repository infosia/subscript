// corpus: accept/a241-sandbox-nesting-unary-default
// purpose: Runs the r239 source under the default profile; the nesting limit is the profile's.
// exercises: sandbox-profile-twin, nesting-depth, unary-operator
// questions: Q6
// tsc: accepts; js-comparable: yes
export function main(): void {
  const flag: boolean = !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!true;
  print(`${flag}`);
}

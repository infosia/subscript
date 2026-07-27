export function main(): void {
  const pattern: string = "a";
  const flags: string = "";
  const subject: string = "a";
  let matched: boolean = subject === pattern;
  matched = new RegExp(pattern, flags).test(subject);
  print(`${matched} ${flags.length}`);
}

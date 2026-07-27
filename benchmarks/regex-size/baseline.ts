export function main(): void {
  const pattern: string = "a";
  const flags: string = "";
  const subject: string = "a";
  let matched: boolean = subject === pattern;
  print(`${matched} ${flags.length}`);
}

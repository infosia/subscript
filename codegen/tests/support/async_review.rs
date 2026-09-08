// Checked-source probes shared by the dev and ship host tests.

pub const BODY: &str = "export async function main(): Promise<void> {
  print(\"m1\");
  await Context.suspend();
  print(\"m2\");
  const arr: i32[] = [];
  print(`x ${arr[5]}`);
}
";

pub const CALLEE: &str = "async function boom(): Promise<void> {
  print(\"boom:start\");
  const arr: i32[] = [];
  print(`x ${arr[5]}`);
}
export async function main(): Promise<void> {
  print(\"m1\");
  await Context.suspend();
  print(\"m2\");
  await boom();
}
";

pub const SETTLED: &str = "async function settled(): Promise<i32> { return 1; }
export async function main(): Promise<void> {
  print(\"m1\");
  const value: i32 = await settled();
  print(\"m2\");
  const arr: i32[] = [];
  print(`x ${arr[5]}`);
}
";

pub const CONTROL: &str = "export async function main(): Promise<void> {
  print(\"m1\");
  await Context.suspend();
  print(\"m2\");
  await Context.suspend();
  print(\"m3\");
}
";

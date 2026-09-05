// corpus: accept/a97-interop-string-field-write
// interpreter: no — calls the synthetic native interop library
// purpose: Expands a leading language string handle to a C string view inside a pointer-passed boundary struct.
// exercises: string-view-field, boundary-struct-pointer, c-layout-scratch, zero-copy-string-view, scalar-offsets
// questions: Q13, C4
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
// compiler.md §28. The C fixture spells `SGPUStringView label` first, then
// uint64_t/bool scalars. Each C observation proves that the call-site scratch
// struct expands `label` to `{data,len}` and copies later fields at C offsets.

export function main(): void {
  const record: SubBoundaryStringRecord = new SubBoundaryStringRecord(
    "view-ok",
    8,
    false,
    42,
    9001,
  );
  print(`${subBoundaryStringCheck(record, 0)}`);
  print(`${subBoundaryStringCheck(record, 1)}`);
  print(`${subBoundaryStringCheck(record, 2)}`);
  print(`${subBoundaryStringCheck(record, 3)}`);
}

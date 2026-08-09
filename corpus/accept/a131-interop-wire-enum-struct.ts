// corpus: accept/a131-interop-wire-enum-struct
// purpose: Stores wire-mapped aliases directly and in a zero-copy pair inside a C boundary struct.
// exercises: CEnum, boundary-struct-member, embedded-array-pair, member-read-validation, switch
// questions: §52, Q32

export function main(): void {
  const record: SubWireModeRecord = new SubWireModeRecord(
    7,
    "m2",
    "bright",
    ["m0", "m1", "m2"],
    99,
  );
  print(`mode=${subWireModeRecordEchoMode(record)}`);
  print(`tone=${subWireModeRecordEchoTone(record)}`);
  print(`element=${subWireModeRecordEchoElement(record, 1)}`);

  subWireModeRecordFill(record);
  switch (record.mode) {
    case "m0":
      print(`filled=m0:${record.mode}`);
      break;
    case "m1":
      print(`filled=m1:${record.mode}`);
      break;
    case "m2":
      print(`filled=m2:${record.mode}`);
      break;
  }
}

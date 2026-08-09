// corpus: trap/t49-wire-enum-struct-unknown-member
// purpose: Reading an unknown C-filled wire alias from a boundary struct traps at the member read.
// exercises: CEnum, boundary-struct-member, C-writeback, unknown-wire-value, trap-stop
// questions: §52, C6
// tier-policy: both tiers trap with kind 24
// expected-trap: wire-enum-unknown-value for SubWireMode value 12345 at the member read

export function main(): void {
  const record: SubWireModeRecord = new SubWireModeRecord(0, "m0", "quiet", [], 0);
  subWireModeRecordFillUnknown(record);
  print("before");
  const mode: SubWireMode = record.mode;
  print(`${mode}`);
}

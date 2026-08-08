// benchmark: bound-call
// Measures 1000 pairs of one array-carrying bound call and one integer-only
// bound call. The backend owns warm-up, sampling, timing, and reporting.

export function main(): void {
  const group: BnBindGroup = bnBindGroupCreate();
  const offsets: u32[] = [7];

  while (bnMoreSamples() !== 0) {
    const t0: i64 = bnNow();
    for (let i: i32 = 0; i < 1000; i += 1) {
      bnSetBindGroup(3, group, offsets);
      bnDraw(1, 2, 3, 4);
    }
    bnRecordSample(t0, bnNow());
  }

  bnReport();
  bnBindGroupRelease(group);
}

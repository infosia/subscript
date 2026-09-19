// corpus: trap/t63-sandbox-sort-quota
// profile: sandbox
// purpose: A sort whose two copies pass the allocation quota reports the `sort` call.
// exercises: sandbox-profile, allocation-quota, array-sort, trap-position
// questions: Q7, compiler section 112
// tier-policy: the two sort copies are the first charge to pass the quota on both tiers, because the entry allocates the array alone before them; the harness sets a quota that holds the array on both tiers and refuses the copies on both
// expected-trap: allocation-quota at the `sort` call, whose position the lowering passes to the runtime

export function main(): void {
  const xs: i32[] = [15, 3, 14, 2, 13, 1, 12, 0, 11, 9, 10, 8, 7, 5, 6, 4];
  xs.sort((a: i32, b: i32): i32 => a - b);
}

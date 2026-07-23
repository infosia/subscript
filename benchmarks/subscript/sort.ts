// benchmark: sort
// Quicksort (median-of-three pivot, recurse-smaller/iterate-larger to bound
// stack depth) of 300000 u32 values seeded by the fixed LCG
// state = state*1664525 + 1013904223. Values are compared as unsigned.
// Checksum: an order-sensitive rolling hash of the sorted array,
// h = h*31 + a[i] (u32 wrap) — permutation-variant, so it detects a
// mis-sort, unlike a plain sum.

const COUNT: i32 = 300000;

function median3(a: u32[], lo: i32, mid: i32, hi: i32): i32 {
  const x: u32 = a[lo];
  const y: u32 = a[mid];
  const z: u32 = a[hi];
  if (x < y) {
    if (y < z) {
      return mid;
    }
    if (x < z) {
      return hi;
    }
    return lo;
  }
  if (x < z) {
    return lo;
  }
  if (y < z) {
    return hi;
  }
  return mid;
}

function quicksort(a: u32[], lo: i32, hi: i32): void {
  let l: i32 = lo;
  let h: i32 = hi;
  while (l < h) {
    const mid: i32 = l + ((h - l) / 2);
    const pivotIndex: i32 = median3(a, l, mid, h);
    let tmp: u32 = a[pivotIndex];
    a[pivotIndex] = a[h];
    a[h] = tmp;
    const pivot: u32 = a[h];
    let store: i32 = l;
    for (let i: i32 = l; i < h; i += 1) {
      if (a[i] < pivot) {
        tmp = a[i];
        a[i] = a[store];
        a[store] = tmp;
        store += 1;
      }
    }
    tmp = a[store];
    a[store] = a[h];
    a[h] = tmp;
    if (store - l < h - store) {
      quicksort(a, l, store - 1);
      l = store + 1;
    } else {
      quicksort(a, store + 1, h);
      h = store - 1;
    }
  }
}

export function main(): void {
  let state: u32 = 0x12345678;
  const a: u32[] = [];
  for (let i: i32 = 0; i < COUNT; i += 1) {
    state = (state * 1664525 + 1013904223) as u32;
    a.push(state);
  }
  quicksort(a, 0, COUNT - 1);
  let h: u32 = 0;
  for (let i: i32 = 0; i < COUNT; i += 1) {
    h = (h * 31 + a[i]) as u32;
  }
  print(`${h}`);
}

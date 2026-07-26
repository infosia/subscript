// benchmark: sort (JS: runs under both jsc and node)
// Quicksort (median-of-three) of 300000 u32 LCG values, compared as unsigned;
// order-sensitive rolling hash checksum h = h*31 + a[i] (u32 via Math.imul/>>>0).
"use strict";

function median3(a, lo, mid, hi) {
  var x = a[lo], y = a[mid], z = a[hi];
  if (x < y) {
    if (y < z) { return mid; }
    if (x < z) { return hi; }
    return lo;
  }
  if (x < z) { return lo; }
  if (y < z) { return hi; }
  return mid;
}

function quicksort(a, lo, hi) {
  var l = lo, h = hi;
  while (l < h) {
    var mid = l + ((h - l) >> 1);
    var pivotIndex = median3(a, l, mid, h);
    var tmp = a[pivotIndex]; a[pivotIndex] = a[h]; a[h] = tmp;
    var pivot = a[h];
    var store = l;
    for (var i = l; i < h; i++) {
      if (a[i] < pivot) {
        tmp = a[i]; a[i] = a[store]; a[store] = tmp;
        store++;
      }
    }
    tmp = a[store]; a[store] = a[h]; a[h] = tmp;
    if (store - l < h - store) { quicksort(a, l, store - 1); l = store + 1; }
    else { quicksort(a, store + 1, h); h = store - 1; }
  }
}

function workload() {
  var COUNT = 300000;
  var state = 0x12345678;
  var a = new Array(COUNT);
  for (var i = 0; i < COUNT; i++) {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    a[i] = state;
  }
  quicksort(a, 0, COUNT - 1);
  var h = 0;
  for (var j = 0; j < COUNT; j++) {
    h = (Math.imul(h, 31) + a[j]) >>> 0;
  }
  return h;
}

var emit = (typeof print === "function") ? print : console.log;
var emitError = (typeof printErr === "function") ? printErr : console.error;
function nowMs() { return performance.now(); }
// Warm-up and timed counts come from argv (node: process.argv; jsc: the
// top-level arguments passed after `--`), using the count as a minimum. Every subject also measures at least
// 200 ms of workload execution and reports the actual warm-up time.
var __argv = (typeof process !== "undefined" && process.argv) ? process.argv.slice(2)
  : (typeof arguments !== "undefined") ? Array.prototype.slice.call(arguments) : [];
(function (argv) {
  var WARMUP = argv.length >= 1 ? (argv[0] | 0) : 3;
  var TIMED = argv.length >= 2 ? (argv[1] | 0) : 11;
  var minimumWarmup = Math.max(WARMUP, 3);
  var warmupMs = 0, warmupIterations = 0;
  var checksum = 0, i;
  while (warmupIterations < minimumWarmup || warmupMs < 200) {
    var warmupStart = nowMs();
    checksum = workload();
    var warmupEnd = nowMs();
    warmupMs += warmupEnd - warmupStart;
    warmupIterations++;
  }
  var times = new Array(TIMED);
  for (i = 0; i < TIMED; i++) {
    var t0 = nowMs();
    checksum = workload();
    var t1 = nowMs();
    times[i] = t1 - t0;
  }
  emitError("warmup " + warmupIterations + " " + (warmupMs / 1000).toFixed(9));
  for (i = 0; i < TIMED; i++) {
    emitError("sample " + i + " " + (times[i] / 1000).toFixed(9));
  }
  times.sort(function (a, b) { return a - b; });
  var mid = TIMED >> 1;
  var median = (TIMED % 2 === 1) ? times[mid] : (times[mid - 1] + times[mid]) / 2;
  emit(String(checksum) + " " + (median / 1000).toFixed(9));
})(__argv);

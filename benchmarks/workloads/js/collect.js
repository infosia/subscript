// benchmark: collect (JS: runs under both jsc and node)
// Build six graphs of 20000 nodes from the fixed LCG. Every node owns four
// unique strings of deliberately unaligned lengths 9/41/105/233. Nodes with
// (state&3)!=0 survive (exactly 15000 per round); the rest and the previous
// survivor graph are dropped before a forced full collection.
// Checksum per surviving node, in reverse build order:
//   checksum = checksum*31 + state + 9 + 41 + 105 + 233 (i32 wrap).
"use strict";

function Node(value, s9, s41, s105, s233, next) {
  this.value = value;
  this.s9 = s9;
  this.s41 = s41;
  this.s105 = s105;
  this.s233 = s233;
  this.next = next;
}

function forceCollection() {
  if (typeof gc === "function") {
    gc();
    return;
  }
  if (typeof $vm === "object" && typeof $vm.gc === "function") {
    $vm.gc();
    return;
  }
  throw new Error("forced collection is unavailable");
}

function workload() {
  var COUNT = 20000, ROUNDS = 6;
  var state = 0x12345678 | 0;
  var checksum = 0;
  var keep = null;

  for (var round = 0; round < ROUNDS; round++) {
    // Dropping keep makes the preceding round's survivor graph reclaimable.
    keep = null;
    var dropped = null;
    var suffix = null, s9 = null, s41 = null, s105 = null, s233 = null;
    var node = null;

    for (var i = 0; i < COUNT; i++) {
      state = (Math.imul(state, 1664525) + 1013904223) | 0;
      var uid = round * COUNT + i;
      suffix = String(uid);
      s9 = suffix.padStart(9, "a");
      s41 = suffix.padStart(41, "b");
      s105 = suffix.padStart(105, "c");
      s233 = suffix.padStart(233, "d");
      if ((state & 3) !== 0) {
        node = new Node(state, s9, s41, s105, s233, keep);
        keep = node;
      } else {
        node = new Node(state, s9, s41, s105, s233, dropped);
        dropped = node;
      }
    }

    dropped = null;
    node = null;
    suffix = s9 = s41 = s105 = s233 = null;
    forceCollection();

    var cursor = keep;
    while (cursor !== null) {
      checksum = (Math.imul(checksum, 31) + cursor.value) | 0;
      checksum = (checksum + cursor.s9.length) | 0;
      checksum = (checksum + cursor.s41.length) | 0;
      checksum = (checksum + cursor.s105.length) | 0;
      checksum = (checksum + cursor.s233.length) | 0;
      cursor = cursor.next;
    }
  }

  keep = null;
  forceCollection();
  return checksum;
}

var emit = (typeof print === "function") ? print : console.log;
var emitError = (typeof printErr === "function") ? printErr : console.error;
function nowMs() { return performance.now(); }
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

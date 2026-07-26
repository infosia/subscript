// benchmark: callbacks (JS: runs under both jsc and node)
// Indexed map/filter/reduce over 1000000 signed i32 LCG values, repeated 20
// times. Every arithmetic step is forced to i32 with |0. The filter removes
// exactly 250000 values per round. Checksum: i32 sum of the reduce results.
"use strict";

function mapValue(value, index) {
  return (value + index) | 0;
}

function keepValue(value, index) {
  return ((value ^ index) & 3) !== 0;
}

function reduceValue(acc, value, index) {
  acc = (acc + value) | 0;
  return (acc + index) | 0;
}

function workload() {
  var COUNT = 1000000, ROUNDS = 20;
  var state = 0x12345678 | 0;
  var input = new Array(COUNT);
  for (var i = 0; i < COUNT; i++) {
    state = (Math.imul(state, 1664525) + 1013904223) | 0;
    input[i] = state;
  }

  var checksum = 0;
  for (var round = 0; round < ROUNDS; round++) {
    var mapped = input.map(mapValue);
    var filtered = mapped.filter(keepValue);
    var reduced = filtered.reduce(reduceValue, 0);
    checksum = (checksum + reduced) | 0;
  }
  return checksum;
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

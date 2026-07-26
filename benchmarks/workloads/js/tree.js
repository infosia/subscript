// benchmark: tree (JS: runs under both jsc and node)
// Build and traverse 30 full binary trees of depth 16; the engine's GC reclaims
// each tree (JS has no manual free). Checksum: node-visit count = 3932130.
"use strict";

function build(depth) {
  if (depth === 0) {
    return { left: null, right: null };
  }
  return { left: build(depth - 1), right: build(depth - 1) };
}

function check(node) {
  if (node.left === null) {
    return 1;
  }
  if (node.right === null) {
    return 1;
  }
  return 1 + check(node.left) + check(node.right);
}

function workload() {
  var DEPTH = 16, COUNT = 30;
  var checksum = 0;
  for (var i = 0; i < COUNT; i++) {
    var root = build(DEPTH);
    checksum += check(root);
    root = null;
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

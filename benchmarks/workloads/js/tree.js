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
// top-level arguments passed after `--`), defaulting to the 3/11 floor, so the
// runner drives every self-timed subject with the same counts it uses for the
// two subscript tiers.
var __argv = (typeof process !== "undefined" && process.argv) ? process.argv.slice(2)
  : (typeof arguments !== "undefined") ? Array.prototype.slice.call(arguments) : [];
(function (argv) {
  var WARMUP = argv.length >= 1 ? (argv[0] | 0) : 3;
  var TIMED = argv.length >= 2 ? (argv[1] | 0) : 11;
  var checksum = 0, i;
  for (i = 0; i < WARMUP; i++) { checksum = workload(); }
  var times = new Array(TIMED);
  for (i = 0; i < TIMED; i++) {
    var t0 = nowMs();
    checksum = workload();
    var t1 = nowMs();
    times[i] = t1 - t0;
  }
  times.sort(function (a, b) { return a - b; });
  var mid = TIMED >> 1;
  var median = (TIMED % 2 === 1) ? times[mid] : (times[mid - 1] + times[mid]) / 2;
  emit(String(checksum) + " " + (median / 1000).toFixed(9));
  emitError("spread " + (times[0] / 1000).toFixed(9) + " "
    + (times[TIMED - 1] / 1000).toFixed(9));
})(__argv);

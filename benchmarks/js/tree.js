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
function nowMs() { return performance.now(); }
(function () {
  var WARMUP = 3, TIMED = 11, checksum = 0, i;
  for (i = 0; i < WARMUP; i++) { checksum = workload(); }
  var times = new Array(TIMED);
  for (i = 0; i < TIMED; i++) {
    var t0 = nowMs();
    checksum = workload();
    var t1 = nowMs();
    times[i] = t1 - t0;
  }
  times.sort(function (a, b) { return a - b; });
  var median = times[(TIMED - 1) >> 1];
  emit(String(checksum) + " " + (median / 1000).toFixed(9));
})();

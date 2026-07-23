// benchmark: queen (JS: runs under both jsc and node)
// Count solutions to 13-queens by bitmask backtracking (32-bit int ops).
"use strict";

function solve(cols, ld, rd, all) {
  if (cols === all) {
    return 1;
  }
  var count = 0;
  var poss = ~(cols | ld | rd) & all;
  while (poss !== 0) {
    var p = poss & (-poss);
    poss = poss - p;
    count += solve(cols | p, (ld | p) << 1, (rd | p) >> 1, all);
  }
  return count;
}

function workload() {
  var seed = [13];
  var bits = seed[0];
  var all = (1 << bits) - 1;
  return solve(0, 0, 0, all);
}

var emit = (typeof print === "function") ? print : console.log;
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
})(__argv);

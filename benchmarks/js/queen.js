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

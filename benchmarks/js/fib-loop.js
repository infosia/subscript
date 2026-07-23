// benchmark: fib-loop (JS: runs under both jsc and node)
// Iterative Fibonacci with a masked feedback dependency on the accumulator to
// defeat closed-form folding; all arithmetic wraps to i32 via |0.
"use strict";

function workload() {
  var INNER = 32, OUTER = 3000000;
  var result = 0;
  for (var iter = 0; iter < OUTER; iter++) {
    var a = iter & 1023;
    var b = 1 + (result & 1023);
    for (var i = 0; i < INNER; i++) {
      var t = (a + b) | 0;
      a = b;
      b = t;
    }
    result = (result + b) | 0;
  }
  return result;
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

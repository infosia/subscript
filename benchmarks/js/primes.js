// benchmark: primes (JS: runs under both jsc and node)
// Count primes up to 500000 by trial division (j*j <= n, no sqrt).
"use strict";

function isPrime(n) {
  if (n < 2) {
    return false;
  }
  for (var j = 2; j * j <= n; j++) {
    if (n % j === 0) {
      return false;
    }
  }
  return true;
}

function workload() {
  var count = 0;
  for (var n = 2; n <= 500000; n++) {
    if (isPrime(n)) {
      count++;
    }
  }
  return count;
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

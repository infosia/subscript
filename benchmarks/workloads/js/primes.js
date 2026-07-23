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

// benchmark: particles (JS: runs under both jsc and node)
// 100000 particles over 1000 steps (velocity += acc*dt; position += velocity*dt),
// dt = 1.0, integer-valued so exact. Checksum: i32-wrapping sum of positions|0.
// Uses two Float64Arrays (contiguous) as the closest analog to subscript's
// packed value-struct array.
"use strict";

function workload() {
  var COUNT = 100000, STEPS = 1000, DT = 1.0;
  var pos = new Float64Array(COUNT);
  var vel = new Float64Array(COUNT);
  for (var step = 0; step < STEPS; step++) {
    for (var i = 0; i < COUNT; i++) {
      var acc = i % 16;
      vel[i] += acc * DT;
      pos[i] += vel[i] * DT;
    }
  }
  var sum = 0;
  for (var j = 0; j < COUNT; j++) {
    sum = (sum + (pos[j] | 0)) | 0;
  }
  return sum;
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

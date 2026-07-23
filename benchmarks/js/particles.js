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

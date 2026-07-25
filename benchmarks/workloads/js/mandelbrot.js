// benchmark: mandelbrot (JS: runs under both jsc and node)
// 800x800 escape grid, escape test x^2 + y^2 >= 4, cap 255, all f64.
"use strict";

function escapes(cx, cy) {
  var zx = 0.0, zy = 0.0;
  for (var i = 0; i < 255; i++) {
    var zx2 = zx * zx;
    var zy2 = zy * zy;
    if (zx2 + zy2 >= 4.0) {
      return i;
    }
    var xy = zx * zy;
    zy = xy + xy + cy;
    zx = zx2 - zy2 + cx;
  }
  return 255;
}

function workload() {
  var GRID = 800;
  var xmin = -2.0, xmax = 0.5, ymin = -1.25, ymax = 1.25;
  var checksum = 0;
  for (var py = 0; py < GRID; py++) {
    var cy = ymin + (ymax - ymin) * py / GRID;
    for (var px = 0; px < GRID; px++) {
      var cx = xmin + (xmax - xmin) * px / GRID;
      checksum += escapes(cx, cy);
    }
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

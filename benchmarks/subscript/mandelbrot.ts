// benchmark: mandelbrot
// Escape-iteration count over an 800x800 grid, escape test x^2 + y^2 >= 4
// (no sqrt), capped at 255. All arithmetic is f64 so every subject computes
// bit-identical escape counts; products are stored in named temporaries and
// only added/subtracted afterwards, so no a*b+c pattern remains to contract.
// Checksum: sum of escape counts as i64.

const GRID: i32 = 800;
const MAX_ITER: i32 = 255;

function escapes(cx: f64, cy: f64): i32 {
  let zx: f64 = 0.0;
  let zy: f64 = 0.0;
  for (let i: i32 = 0; i < MAX_ITER; i += 1) {
    const zx2: f64 = zx * zx;
    const zy2: f64 = zy * zy;
    if (zx2 + zy2 >= 4.0) {
      return i;
    }
    const xy: f64 = zx * zy;
    zy = xy + xy + cy;
    zx = zx2 - zy2 + cx;
  }
  return MAX_ITER;
}

export function main(): void {
  const xmin: f64 = -2.0;
  const xmax: f64 = 0.5;
  const ymin: f64 = -1.25;
  const ymax: f64 = 1.25;
  let checksum: i64 = 0;
  for (let py: i32 = 0; py < GRID; py += 1) {
    const cy: f64 = ymin + (ymax - ymin) * (py as f64) / (GRID as f64);
    for (let px: i32 = 0; px < GRID; px += 1) {
      const cx: f64 = xmin + (xmax - xmin) * (px as f64) / (GRID as f64);
      checksum += (escapes(cx, cy) as i64);
    }
  }
  print(`${checksum}`);
}

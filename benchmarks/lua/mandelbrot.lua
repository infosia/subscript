-- benchmark: mandelbrot (LuaJIT)
-- 800x800 escape grid, escape test x^2 + y^2 >= 4, cap 255, all f64.

local function escapes(cx, cy)
  local zx, zy = 0.0, 0.0
  for i = 0, 254 do
    local zx2 = zx * zx
    local zy2 = zy * zy
    if zx2 + zy2 >= 4.0 then
      return i
    end
    local xy = zx * zy
    zy = xy + xy + cy
    zx = zx2 - zy2 + cx
  end
  return 255
end

local function workload()
  local GRID = 800
  local xmin, xmax, ymin, ymax = -2.0, 0.5, -1.25, 1.25
  local checksum = 0
  for py = 0, GRID - 1 do
    local cy = ymin + (ymax - ymin) * py / GRID
    for px = 0, GRID - 1 do
      local cx = xmin + (xmax - xmin) * px / GRID
      checksum = checksum + escapes(cx, cy)
    end
  end
  return checksum
end

local WARMUP, TIMED = 3, 11
local checksum = 0
for _ = 1, WARMUP do checksum = workload() end
local times = {}
for i = 1, TIMED do
  local t0 = os.clock()
  checksum = workload()
  local t1 = os.clock()
  times[i] = t1 - t0
end
table.sort(times)
local median = times[math.floor((TIMED + 1) / 2)]
io.write(string.format("%d %.9f\n", checksum, median))

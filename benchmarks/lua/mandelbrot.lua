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

-- Warm-up and timed counts come from argv (arg[1] arg[2]), defaulting to the
-- 3/11 floor, so every self-timed subject uses the same counts as the runner.
local WARMUP = tonumber(arg and arg[1]) or 3
local TIMED = tonumber(arg and arg[2]) or 11
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
local mid = math.floor(TIMED / 2)
local median
if TIMED % 2 == 1 then
  median = times[mid + 1]
else
  median = (times[mid] + times[mid + 1]) / 2
end
io.write(string.format("%d %.9f\n", checksum, median))

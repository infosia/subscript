-- benchmark: particles (LuaJIT)
-- 100000 particles over 1000 steps (velocity += acc*dt; position += velocity*dt),
-- dt = 1.0, integer-valued so exact. Checksum: i32-wrapping sum of positions
-- truncated to i32. Parallel arrays are the closest analog to subscript's packed
-- value-struct array.

local bit = require("bit")
local tobit = bit.tobit

local function workload()
  local COUNT, STEPS, DT = 100000, 1000, 1.0
  local pos, vel = {}, {}
  for i = 0, COUNT - 1 do
    pos[i] = 0.0
    vel[i] = 0.0
  end
  for _ = 1, STEPS do
    for i = 0, COUNT - 1 do
      local acc = (i % 16) + 0.0
      vel[i] = vel[i] + acc * DT
      pos[i] = pos[i] + vel[i] * DT
    end
  end
  local sum = 0
  for i = 0, COUNT - 1 do
    sum = tobit(sum + pos[i])
  end
  return sum
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

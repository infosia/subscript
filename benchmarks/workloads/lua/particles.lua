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

-- The argv warm-up count is a minimum. Every subject also measures at least
-- 200 ms of workload execution and reports the actual warm-up time.
local WARMUP = tonumber(arg and arg[1]) or 3
local TIMED = tonumber(arg and arg[2]) or 11
local minimum_warmup = math.max(WARMUP, 3)
local warmup_seconds, warmup_iterations = 0.0, 0
local checksum = 0
while warmup_iterations < minimum_warmup or warmup_seconds < 0.200 do
  local t0 = os.clock()
  checksum = workload()
  local t1 = os.clock()
  warmup_seconds = warmup_seconds + (t1 - t0)
  warmup_iterations = warmup_iterations + 1
end
local times = {}
for i = 1, TIMED do
  local t0 = os.clock()
  checksum = workload()
  local t1 = os.clock()
  times[i] = t1 - t0
end
io.stderr:write(string.format("warmup %d %.9f\n", warmup_iterations, warmup_seconds))
for i = 1, TIMED do
  io.stderr:write(string.format("sample %d %.9f\n", i - 1, times[i]))
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

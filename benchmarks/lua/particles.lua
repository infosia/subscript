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

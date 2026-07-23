-- benchmark: fib-loop (LuaJIT)
-- Iterative Fibonacci with a masked feedback dependency on the accumulator to
-- defeat closed-form folding; all arithmetic wraps to i32 via the bit library.

local bit = require("bit")
local band, tobit = bit.band, bit.tobit

local function workload()
  local INNER, OUTER = 32, 3000000
  local result = 0
  for iter = 0, OUTER - 1 do
    local a = band(iter, 1023)
    local b = 1 + band(result, 1023)
    for _ = 1, INNER do
      local t = tobit(a + b)
      a = b
      b = t
    end
    result = tobit(result + b)
  end
  return result
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

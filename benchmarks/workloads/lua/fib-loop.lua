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

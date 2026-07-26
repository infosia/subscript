-- benchmark: fib-recursive (LuaJIT)
-- Naive recursive Fibonacci, fib(31) = 1346269. The seed is read from a table
-- so the recursion is not constant-folded.

local function fib(n)
  if n < 2 then
    return n
  end
  return fib(n - 1) + fib(n - 2)
end

local function workload()
  local seed = { [0] = 31 }
  return fib(seed[0])
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

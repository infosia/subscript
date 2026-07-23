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

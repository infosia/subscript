-- benchmark: queen (LuaJIT)
-- Count solutions to 13-queens by bitmask backtracking (bit library, 32-bit).

local bit = require("bit")
local band, bor, bnot, lshift, rshift = bit.band, bit.bor, bit.bnot, bit.lshift, bit.rshift

local function solve(cols, ld, rd, all)
  if cols == all then
    return 1
  end
  local count = 0
  local poss = band(bnot(bor(cols, bor(ld, rd))), all)
  while poss ~= 0 do
    local p = band(poss, -poss)
    poss = poss - p
    count = count + solve(bor(cols, p), lshift(bor(ld, p), 1), rshift(bor(rd, p), 1), all)
  end
  return count
end

local function workload()
  local seed = { [0] = 13 }
  local bits = seed[0]
  local all = lshift(1, bits) - 1
  return solve(0, 0, 0, all)
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

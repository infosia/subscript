-- benchmark: callbacks (LuaJIT)
-- Loop spelling of indexed map/filter/reduce over 1000000 signed i32 LCG
-- values, repeated 20 times. Every arithmetic step is forced to i32 with
-- bit.tobit. The filter removes exactly 250000 values per round.

local bit = require("bit")
local band, bxor, tobit = bit.band, bit.bxor, bit.tobit
local TWO32 = 4294967296

local function workload()
  local COUNT, ROUNDS = 1000000, 20
  local state = tobit(0x12345678)
  local input = {}
  for i = 0, COUNT - 1 do
    state = tobit((state * 1664525 + 1013904223) % TWO32)
    input[i] = state
  end

  local checksum = 0
  for _ = 1, ROUNDS do
    local mapped = {}
    for i = 0, COUNT - 1 do
      mapped[i] = tobit(input[i] + i)
    end

    local filtered = {}
    local kept = 0
    for i = 0, COUNT - 1 do
      local value = mapped[i]
      if band(bxor(value, i), 3) ~= 0 then
        filtered[kept] = value
        kept = kept + 1
      end
    end

    local reduced = 0
    for i = 0, kept - 1 do
      reduced = tobit(reduced + filtered[i])
      reduced = tobit(reduced + i)
    end
    checksum = tobit(checksum + reduced)
  end
  return checksum
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

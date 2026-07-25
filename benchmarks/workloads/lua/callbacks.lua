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
io.stderr:write(string.format("spread %.9f %.9f\n", times[1], times[TIMED]))

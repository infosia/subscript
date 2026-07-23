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

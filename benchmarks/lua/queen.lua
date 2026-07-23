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

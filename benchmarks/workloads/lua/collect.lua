-- benchmark: collect (LuaJIT)
-- Build six graphs of 20000 nodes from the fixed LCG. Every node owns four
-- unique strings of deliberately unaligned lengths 9/41/105/233. Nodes with
-- (state&3)~=0 survive (exactly 15000 per round); the rest and the previous
-- survivor graph are dropped before collectgarbage("collect").
-- Checksum per surviving node, in reverse build order:
--   checksum = checksum*31 + state + 9 + 41 + 105 + 233 (i32 wrap).

local bit = require("bit")
local band, tobit = bit.band, bit.tobit
local TWO32 = 4294967296

local function padded(uid, len, pad)
  local suffix = tostring(uid)
  return string.rep(pad, len - #suffix) .. suffix
end

local function workload()
  local COUNT, ROUNDS = 20000, 6
  local state = tobit(0x12345678)
  local checksum = 0
  local keep = nil

  for round = 0, ROUNDS - 1 do
    -- Dropping keep makes the preceding round's survivor graph reclaimable.
    keep = nil
    local dropped = nil

    for i = 0, COUNT - 1 do
      state = tobit((state * 1664525 + 1013904223) % TWO32)
      local uid = round * COUNT + i
      local s9 = padded(uid, 9, "a")
      local s41 = padded(uid, 41, "b")
      local s105 = padded(uid, 105, "c")
      local s233 = padded(uid, 233, "d")
      if band(state, 3) ~= 0 then
        keep = { state, s9, s41, s105, s233, keep }
      else
        dropped = { state, s9, s41, s105, s233, dropped }
      end
    end

    dropped = nil
    collectgarbage("collect")

    local cursor = keep
    while cursor ~= nil do
      checksum = tobit(checksum * 31 + cursor[1])
      checksum = tobit(checksum + #cursor[2])
      checksum = tobit(checksum + #cursor[3])
      checksum = tobit(checksum + #cursor[4])
      checksum = tobit(checksum + #cursor[5])
      cursor = cursor[6]
    end
  end

  keep = nil
  collectgarbage("collect")
  return checksum
end

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

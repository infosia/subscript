-- benchmark: tree (LuaJIT)
-- Build and traverse 30 full binary trees of depth 16; LuaJIT's GC reclaims each
-- tree (Lua has no manual free). Checksum: node-visit count = 3932130.

local function build(depth)
  if depth == 0 then
    return {}
  end
  return { left = build(depth - 1), right = build(depth - 1) }
end

local function check(node)
  if node.left == nil then
    return 1
  end
  if node.right == nil then
    return 1
  end
  return 1 + check(node.left) + check(node.right)
end

local function workload()
  local DEPTH, COUNT = 16, 30
  local checksum = 0
  for _ = 1, COUNT do
    local root = build(DEPTH)
    checksum = checksum + check(root)
    root = nil
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

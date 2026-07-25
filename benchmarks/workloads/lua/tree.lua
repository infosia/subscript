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

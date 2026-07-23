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

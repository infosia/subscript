-- benchmark: sort (LuaJIT)
-- Quicksort (median-of-three) of 300000 u32 LCG values, compared as unsigned;
-- order-sensitive rolling hash checksum h = h*31 + a[i] (u32 via mod 2^32).
-- The LCG products stay below 2^53, so the modular arithmetic is exact.

local floor = math.floor
local TWO32 = 4294967296

local function median3(a, lo, mid, hi)
  local x, y, z = a[lo], a[mid], a[hi]
  if x < y then
    if y < z then return mid end
    if x < z then return hi end
    return lo
  end
  if x < z then return lo end
  if y < z then return hi end
  return mid
end

local function quicksort(a, lo, hi)
  local l, h = lo, hi
  while l < h do
    local mid = l + floor((h - l) / 2)
    local pivot_index = median3(a, l, mid, h)
    local tmp = a[pivot_index]; a[pivot_index] = a[h]; a[h] = tmp
    local pivot = a[h]
    local store = l
    for i = l, h - 1 do
      if a[i] < pivot then
        tmp = a[i]; a[i] = a[store]; a[store] = tmp
        store = store + 1
      end
    end
    tmp = a[store]; a[store] = a[h]; a[h] = tmp
    if store - l < h - store then
      quicksort(a, l, store - 1); l = store + 1
    else
      quicksort(a, store + 1, h); h = store - 1
    end
  end
end

local function workload()
  local COUNT = 300000
  local state = 0x12345678
  local a = {}
  for i = 0, COUNT - 1 do
    state = (state * 1664525 + 1013904223) % TWO32
    a[i] = state
  end
  quicksort(a, 0, COUNT - 1)
  local h = 0
  for i = 0, COUNT - 1 do
    h = (h * 31 + a[i]) % TWO32
  end
  return h
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
io.write(string.format("%.0f %.9f\n", checksum, median))

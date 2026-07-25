-- benchmark: primes (LuaJIT)
-- Count primes up to 500000 by trial division (j*j <= n, no sqrt).

local function is_prime(n)
  if n < 2 then
    return false
  end
  local j = 2
  while j * j <= n do
    if n % j == 0 then
      return false
    end
    j = j + 1
  end
  return true
end

local function workload()
  local count = 0
  for n = 2, 500000 do
    if is_prime(n) then
      count = count + 1
    end
  end
  return count
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

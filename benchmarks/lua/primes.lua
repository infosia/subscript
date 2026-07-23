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

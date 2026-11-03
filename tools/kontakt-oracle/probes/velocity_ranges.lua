-- Velocity/key geometry census of a loaded instrument, from Kontakt's own API.
-- Answers, for the instrument in ORACLE_INPUTS[1]: how many zones Kontakt thinks
-- there are, the distribution of its stored velocity ranges, how many admit
-- velocity 100, and — for key 60 — which zones admit the note and which sample
-- each one plays. Nothing here assumes the file layout: every value is Kontakt's.
local kt = Kontakt
local out = assert(io.open(ORACLE_OUT, "w"))
local function log(t) out:write(t .. "\n") out:flush() end

local function basename(p)
  return (tostring(p):gsub("\\", "/"):match("([^/]+)$") or tostring(p))
end

kt.reset_multi()
local idx = kt.load_instrument(ORACLE_INPUTS[1])
if math.type(idx) ~= "integer" then
  log('{"load_error":"' .. tostring(idx):gsub('"', "'") .. '"}')
  log('{"done":true}')
  out:close()
  return
end
local nz = kt.get_num_zones(idx)
local ng = kt.get_num_groups(idx)
log(string.format('{"num_zones":%d,"num_groups":%d}', nz, ng))

local pairs_count = {}
local covers100 = 0
local key60 = {}
local zone_kind = {}
for z = 0, nz - 1 do
  local g = kt.get_zone_geometry(idx, z)
  if type(g) == "table" then
    local k = string.format("%d..%d", g.low_velocity, g.high_velocity)
    pairs_count[k] = (pairs_count[k] or 0) + 1
    if g.low_velocity <= 100 and 100 <= g.high_velocity then covers100 = covers100 + 1 end
    if g.low_key <= 60 and 60 <= g.high_key then
      key60[#key60 + 1] = {
        zone = z, group = kt.get_zone_group(idx, z),
        lo = g.low_velocity, hi = g.high_velocity,
        lk = g.low_key, hk = g.high_key, root = g.root_key,
        lvf = g.low_velocity_fade, hvf = g.high_velocity_fade,
        sample = basename(kt.get_zone_sample(idx, z)),
        frames = kt.get_zone_sample_frames(idx, z),
      }
    end
  end
end
log(string.format('{"zones_covering_velocity_100":%d,"zones_covering_key_60":%d}', covers100, #key60))

local keys = {}
for k in pairs(pairs_count) do keys[#keys + 1] = k end
table.sort(keys, function(a, b)
  local al = tonumber(a:match("^(%d+)")) local bl = tonumber(b:match("^(%d+)"))
  if al ~= bl then return al < bl end
  return a < b
end)
local parts = {}
for _, k in ipairs(keys) do parts[#parts + 1] = string.format('"%s":%d', k, pairs_count[k]) end
log('{"velocity_pairs":{' .. table.concat(parts, ",") .. '}}')
for _, r in ipairs(key60) do
  log(string.format(
    '{"key60":{"zone":%d,"group":%d,"vel":%d,"vel_hi":%d,"key":%d,"key_hi":%d,"root":%d,"vfade":%d,"vfade_hi":%d,"frames":%d,"sample":"%s"}}',
    r.zone, r.group, r.lo, r.hi, r.lk, r.hk, r.root, r.lvf, r.hvf, r.frames,
    (r.sample or ""):gsub('"', "'")))
end
log('{"done":true}')
out:close()

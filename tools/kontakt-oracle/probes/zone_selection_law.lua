-- Does Kontakt select a zone by velocity-range membership, or does it fall back
-- to a nearest/lowest/highest layer when no zone admits the note?
--
-- Builds the question out of a real instrument instead of importing samples:
-- `add_zone` with a sample path blocks on Kontakt 8.14.1 (it needs a dialog),
-- but an existing instrument's zones can be re-keyed and re-ranged through the
-- API, and saved. `05 Breaths.nki` has one zone per key, which makes the two
-- zones below the only ones within reach of key 60:
--
--   band A: keys 60..60, velocity 1..60    (root 50, so it sounds ten semitones up)
--   band B: keys 60..60, velocity 100..127 (root 70, ten semitones down)
--   every other zone: keys 0..0
--
-- Then a render at key 60 decides the law: 30 and 60 are inside A, 100 and 127
-- inside B, and 64, 80 and 99 are outside both — where a fallback would sound
-- A or B and membership is silence. Measured 2026-11-03 (Kontakt 8.14.1):
-- 30 = −81.5 dBFS, 60 = −76.9, 64 = −106.7, 80 = −180.0, 99 = −180.0,
-- 100 = −40.4, 127 = −37.4, and keys 59 and 61 (no zone) = −180.0. So: silence,
-- on both axes. docs/re/velocity-selection-2026-11-03.md
--
--   python3 tools/oracle-mcp/client.py '{"tool":"oracle_dump_script","arguments":{
--     "nki":"<…>/05 Breaths.nki",
--     "probe":"tools/kontakt-oracle/probes/zone_selection_law.lua"}}'
local kt = Kontakt
local out = assert(io.open(ORACLE_OUT, "w"))
local function log(t) out:write(t .. "\n") out:flush() end
local dir = ORACLE_OUT:match("^(.*[\\/])")

kt.reset_multi()
local inst = kt.load_instrument(ORACLE_INPUTS[1])
if math.type(inst) ~= "integer" then
  log('{"load_error":"' .. tostring(inst):gsub('"', "'") .. '"}')
  log('{"done":true}'); out:close(); return
end
local nz = kt.get_num_zones(inst)
log(string.format('{"loaded":true,"zones":%d,"groups":%d}', nz, kt.get_num_groups(inst)))

-- Which zone owns which key, before touching anything.
local by_key = {}
for z = 0, nz - 1 do
  local g = kt.get_zone_geometry(inst, z)
  if g.low_key == g.high_key then by_key[g.low_key] = z end
end
log(string.format('{"zones":{"key50":%d,"key60":%d,"key70":%d}}',
  by_key[50] or -1, by_key[60] or -1, by_key[70] or -1))

-- Kontakt validates each setter against the other bound, so move whichever
-- bound keeps low <= high first.
local function set_span(get, set_lo, set_hi, z, lo, hi)
  local cur = get(z)
  if lo > cur[2] then
    set_hi(z, hi); set_lo(z, lo)
  else
    set_lo(z, lo); set_hi(z, hi)
  end
end

local function keys(inst, z, lo, hi)
  set_span(function(zz) local g = kt.get_zone_geometry(inst, zz) return { g.low_key, g.high_key } end,
    function(zz, v) kt.set_zone_low_key(inst, zz, v) end,
    function(zz, v) kt.set_zone_high_key(inst, zz, v) end, z, lo, hi)
end

local function vels(inst, z, lo, hi)
  set_span(function(zz) local g = kt.get_zone_geometry(inst, zz) return { g.low_velocity, g.high_velocity } end,
    function(zz, v) kt.set_zone_low_velocity(inst, zz, v) end,
    function(zz, v) kt.set_zone_high_velocity(inst, zz, v) end, z, lo, hi)
end

for z = 0, nz - 1 do
  if z ~= by_key[50] and z ~= by_key[70] then keys(inst, z, 0, 0) end
end
log('{"step":"cleared"}')
keys(inst, by_key[50], 60, 60)
vels(inst, by_key[50], 1, 60)
log('{"step":"band_a"}')
keys(inst, by_key[70], 60, 60)
vels(inst, by_key[70], 100, 127)
log('{"step":"band_b"}')

for _, z in ipairs({ by_key[50], by_key[70] }) do
  local g = kt.get_zone_geometry(inst, z)
  log(string.format(
    '{"zone":%d,"group":%d,"vel":"%d..%d","key":"%d..%d","root":%d,"sample":"%s"}',
    z, kt.get_zone_group(inst, z), g.low_velocity, g.high_velocity, g.low_key, g.high_key,
    g.root_key, tostring(kt.get_zone_sample(inst, z):gsub("\\", "/"):match("([^/]+)$"))))
end
local ok, err = pcall(kt.save_instrument, inst, dir .. "selection_law.nki", {})
log('{"saved":"selection_law.nki","ok":' .. tostring(ok) .. ',"err":"' .. tostring(err):gsub('"', "'") .. '"}')
log('{"done":true}')
out:close()

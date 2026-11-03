-- What a large value does in the per-event parameters that accept small ones:
-- is there a clamp, and where, in Kontakt 8.14.1?
--
-- `event_par_roundtrip.lua` measured 12, 13 and 14 round-tripping 500, 501 and
-- 502, and 18 round-tripping 7 (`event_par_extended.lua`) but not 506. This
-- probe writes 7, 100, 127, 128, 500 and 250000 into parameter 18 and asks
-- whether each reads back unchanged.
--
--   tools/oracle-mcp/client.py '{"tool":"oracle_dump_script","arguments":{
--     "nki":"<a .wav>","probe":"tools/kontakt-oracle/probes/event_par_clamp.lua"}}'
local kt = Kontakt
local out = assert(io.open(ORACLE_OUT, "w"))
local function log(t) out:write(t .. "\n") out:flush() end
local function jstr(v)
  local s = tostring(v)
  s = s:gsub("\\", "\\\\"):gsub('"', '\\"'):gsub("\n", "\\n"):gsub("\r", "\\r"):gsub("\t", "\\t")
  return '"' .. s .. '"'
end
local dir = ORACLE_OUT:match("^(.*[\\/])") or ""

kt.reset_multi()
local inst = kt.add_instrument()
local z = kt.add_zone(inst, 0, ORACLE_INPUTS[1])
kt.set_zone_geometry(inst, z, { root_key = 60, low_key = 0, high_key = 127 })

local VALUES = { 7, 100, 127, 128, 500, 250000 }
local src = { "on init", "  declare $a", "end on", "on note" }
for i, value in ipairs(VALUES) do
  local key = 59 + i
  table.insert(src, string.format("  if ($EVENT_NOTE = %d)", key))
  table.insert(src, string.format("    set_event_par($EVENT_ID, 18, %d)", value))
  table.insert(src, "    $a := get_event_par($EVENT_ID, 18)")
  table.insert(src, string.format([[
    if ($a = %d)
      set_engine_par($ENGINE_PAR_VOLUME, 630000, -1, -1, -1)
    else
      set_engine_par($ENGINE_PAR_VOLUME, 0, -1, -1, -1)
    end if]], value))
  table.insert(src, "  end if")
  log(string.format('{"key":%d,"par":18,"value":%d}', key, value))
end
table.insert(src, "end on")
local text = table.concat(src, "\n") .. "\n"
local path = dir .. "event_par_clamp.ksp"
local fh = assert(io.open(path, "w")); fh:write(text); fh:close()
local ok, err = pcall(kt.set_instrument_script_source, inst, 0, text)
log(string.format('{"compile":%s,"err":%s}', tostring(ok), jstr(err or "")))
local saved, serr = pcall(kt.save_instrument, inst, dir .. "event_par_clamp.nki", {})
log(string.format('{"saved":%s,"nki":%s,"err":%s}', tostring(saved),
    jstr(dir .. "event_par_clamp.nki"), jstr(serr or "")))
log('{"done":true}')
out:close()

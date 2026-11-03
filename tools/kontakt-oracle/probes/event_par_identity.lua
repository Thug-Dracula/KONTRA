-- What the per-event parameters numbered 12..20 are, in Kontakt 8.14.1.
--
-- `event_par_extended.lua` measured that 18 and 19 round-trip a plain integer on a
-- note and on a note `play_note` created, that a fresh event reads 0, and that 12
-- does not round-trip. This probe asks whether 12..19 are the "from script"
-- modulation values (`set_event_par_arr(..., $EVENT_PAR_MOD_VALUE_ID, ..., index)`),
-- whether the free range continues past 19, and whether negative values survive.
--
-- Answer per key, audible for "yes", silent for "no" (instrument volume 630000/0):
--
--   key 60  arr(MV, 250000, 0..3)  -> get_event_par(id, 12 + i)   = 250000   (four keys 60..63)
--   key 64  arr(MV, 250000, 6)     -> get_event_par(id, 18)       = 250000
--   key 65  arr(MV, 250000, 7)     -> get_event_par(id, 19)       = 250000
--   key 66  set_event_par(id, 20, 7)      -> get_event_par(id, 20)      = 7
--   key 67  set_event_par(id, 18, 250000) -> arr(MV, 6)                 = 250000
--   key 68  set_event_par(id, 19, -7)     -> get_event_par(id, 19)      = -7
--   key 69  negative control: set 18 = 7, ask whether it reads 8 (must be silent)
--
--   tools/oracle-mcp/client.py '{"tool":"oracle_dump_script","arguments":{
--     "nki":"<a .wav>","probe":"tools/kontakt-oracle/probes/event_par_identity.lua"}}'
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

local answer = function(expr)
  return string.format([[
    if (%s)
      set_engine_par($ENGINE_PAR_VOLUME, 630000, -1, -1, -1)
    else
      set_engine_par($ENGINE_PAR_VOLUME, 0, -1, -1, -1)
    end if]], expr)
end

local src = { "on init", "  declare $a", "end on", "on note" }
for i = 0, 3 do
  local key = 60 + i
  table.insert(src, string.format("  if ($EVENT_NOTE = %d)", key))
  table.insert(src, string.format(
    "    set_event_par_arr($EVENT_ID, $EVENT_PAR_MOD_VALUE_ID, 250000, %d)", i))
  table.insert(src, string.format("    $a := get_event_par($EVENT_ID, %d)", 12 + i))
  table.insert(src, answer("$a = 250000"))
  table.insert(src, "  end if")
end
-- mod value 6 and 7 against 18 and 19
for _, pair in ipairs({ { 6, 18, 64 }, { 7, 19, 65 } }) do
  local idx, par, key = pair[1], pair[2], pair[3]
  table.insert(src, string.format("  if ($EVENT_NOTE = %d)", key))
  table.insert(src, string.format(
    "    set_event_par_arr($EVENT_ID, $EVENT_PAR_MOD_VALUE_ID, 250000, %d)", idx))
  table.insert(src, string.format("    $a := get_event_par($EVENT_ID, %d)", par))
  table.insert(src, answer("$a = 250000"))
  table.insert(src, "  end if")
end
-- does the free range continue past 19
table.insert(src, "  if ($EVENT_NOTE = 66)")
table.insert(src, "    set_event_par($EVENT_ID, 20, 7)")
table.insert(src, "    $a := get_event_par($EVENT_ID, 20)")
table.insert(src, answer("$a = 7"))
table.insert(src, "  end if")
-- the mirror: does setting 18 move mod value 6
table.insert(src, "  if ($EVENT_NOTE = 67)")
table.insert(src, "    set_event_par($EVENT_ID, 18, 250000)")
table.insert(src, "    $a := get_event_par_arr($EVENT_ID, $EVENT_PAR_MOD_VALUE_ID, 6)")
table.insert(src, answer("$a = 250000"))
table.insert(src, "  end if")
-- negative values survive
table.insert(src, "  if ($EVENT_NOTE = 68)")
table.insert(src, "    set_event_par($EVENT_ID, 19, -7)")
table.insert(src, "    $a := get_event_par($EVENT_ID, 19)")
table.insert(src, answer("$a = -7"))
table.insert(src, "  end if")
-- negative control
table.insert(src, "  if ($EVENT_NOTE = 69)")
table.insert(src, "    set_event_par($EVENT_ID, 18, 7)")
table.insert(src, "    $a := get_event_par($EVENT_ID, 18)")
table.insert(src, answer("$a = 8"))
table.insert(src, "  end if")
table.insert(src, "end on")
local text = table.concat(src, "\n") .. "\n"

local path = dir .. "event_par_identity.ksp"
local fh = assert(io.open(path, "w")); fh:write(text); fh:close()
log(string.format('{"ksp":%s,"ksp_path":%s}', jstr(text), jstr(path)))
local ok, err = pcall(kt.set_instrument_script_source, inst, 0, text)
log(string.format('{"compile":%s,"err":%s}', tostring(ok), jstr(err or "")))
local saved, serr = pcall(kt.save_instrument, inst, dir .. "event_par_identity.nki", {})
log(string.format('{"saved":%s,"nki":%s,"err":%s}', tostring(saved),
    jstr(dir .. "event_par_identity.nki"), jstr(serr or "")))
log('{"done":true}')
out:close()

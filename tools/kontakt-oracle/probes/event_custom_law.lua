-- Does `set_event_par_arr(id, $EVENT_PAR_CUSTOM, value, index)` read back through
-- `get_event_par_arr(id, $EVENT_PAR_CUSTOM, index)`, and is `$EVENT_PAR_CUSTOM` 28?
--
-- A note event is the only place `$EVENT_ID` is legal, and a disposable probe
-- Kontakt launched with a Lua script hears no MIDI, so this probe does not try to
-- play the note. It builds a synthetic instrument whose script answers one
-- question per key — audible for "yes", silent for "no" — and saves it beside the
-- output. Load the saved `.nki` in the main oracle session, `oracle_render` the
-- four keys, and decode which windows sound:
--
--   key 60  get(symbol, 1) = 4242 after set(symbol, 4242, 1)
--   key 62  get(28,     1) = 4242 after set(symbol, 4242, 1)   (symbol == 28)
--   key 64  get(symbol, 0) = 0 on a fresh event
--   key 65  get(symbol, 0) = 7 after set(28, 7, 0)
--   key 66  negative control: set 4242, ask whether the read is 1234 (must be silent)
--
--   tools/oracle-mcp/client.py '{"tool":"oracle_dump_script","arguments":{
--     "nki":"<a .wav>","probe":"tools/kontakt-oracle/probes/event_custom_law.lua"}}'
--
-- Measured (Kontakt 8.14.1, 2026-11-03): see the JSON this writes and the render
-- the main session makes of its event_custom_law.nki.
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
-- A real sample for the zone: this probe takes its path as ORACLE_INPUTS[1]
-- (the launcher's `nki` argument) and never loads it as an instrument.
local sample = ORACLE_INPUTS[1]
local z = kt.add_zone(inst, 0, sample)
kt.set_zone_geometry(inst, z, { root_key = 60, low_key = 0, high_key = 127 })
log(string.format('{"zone":%s,"sample":%s}', tostring(z), jstr(sample)))

local answer = function(expr)
  return string.format([[
    if (%s)
      set_engine_par($ENGINE_PAR_VOLUME, 630000, -1, -1, -1)
    else
      set_engine_par($ENGINE_PAR_VOLUME, 0, -1, -1, -1)
    end if]], expr)
end
local text = table.concat({
  "on init",
  "  declare $a",
  "  declare $b",
  "  declare $c",
  "  declare $d",
  "end on",
  "on note",
  "  if ($EVENT_NOTE = 60)",
  "    set_event_par_arr($EVENT_ID, $EVENT_PAR_CUSTOM, 4242, 1)",
  "    $a := get_event_par_arr($EVENT_ID, $EVENT_PAR_CUSTOM, 1)",
  answer("$a = 4242"),
  "  end if",
  "  if ($EVENT_NOTE = 62)",
  "    set_event_par_arr($EVENT_ID, $EVENT_PAR_CUSTOM, 4242, 1)",
  "    $b := get_event_par_arr($EVENT_ID, 28, 1)",
  answer("$b = 4242"),
  "  end if",
  "  if ($EVENT_NOTE = 64)",
  "    $c := get_event_par_arr($EVENT_ID, $EVENT_PAR_CUSTOM, 0)",
  answer("$c = 0"),
  "  end if",
  "  if ($EVENT_NOTE = 65)",
  "    set_event_par_arr($EVENT_ID, 28, 7, 0)",
  "    $d := get_event_par_arr($EVENT_ID, $EVENT_PAR_CUSTOM, 0)",
  answer("$d = 7"),
  "  end if",
  "  if ($EVENT_NOTE = 66)",
  "    set_event_par_arr($EVENT_ID, $EVENT_PAR_CUSTOM, 4242, 1)",
  "    $d := get_event_par_arr($EVENT_ID, $EVENT_PAR_CUSTOM, 1)",
  answer("$d = 1234"),
  "  end if",
  "end on",
}, "\n") .. "\n"
local path = dir .. "event_custom_law.ksp"
local fh = assert(io.open(path, "w")); fh:write(text); fh:close()
log(string.format('{"ksp":%s,"ksp_path":%s}', jstr(text), jstr(path)))
local ok, err = pcall(kt.set_instrument_script_source, inst, 0, text)
log(string.format('{"compile":%s,"err":%s}', tostring(ok), jstr(err or "")))
local saved, serr = pcall(kt.save_instrument, inst, dir .. "event_custom_law.nki", {})
log(string.format('{"saved":%s,"nki":%s,"err":%s}', tostring(saved),
    jstr(dir .. "event_custom_law.nki"), jstr(serr or "")))
log('{"done":true}')
out:close()

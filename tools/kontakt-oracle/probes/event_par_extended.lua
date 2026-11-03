-- Which per-event parameter numbers Kontakt 8.14.1 accepts besides `$EVENT_PAR_0..3`.
--
-- The Evolution bass family (95 instruments of the gap corpus) writes the note's
-- string number with `set_event_par($id, 18, string)` in one slot and reads it in
-- the next. This probe asks whether 18 (and neighbours 12/19) are legal per-note
-- parameters, whether they round-trip unclamped, whether they survive on a note
-- `play_note` created (read from a later slot), and whether 18 is the same storage
-- as `set_event_par_arr(..., $EVENT_PAR_MOD_VALUE_ID, ..., 6)`.
--
-- Answer per key, audible for "yes", silent for "no" (instrument volume 630000/0):
--
--   key 60  set_event_par(id, 18, 7)        -> get_event_par(id, 18) = 7
--   key 61  set_event_par(id, 12, 13)       -> get_event_par(id, 12) = 13
--   key 62  fresh event                     -> get_event_par(id, 18) = 0
--   key 63  set_event_par(id, 19, 5)        -> get_event_par(id, 19) = 5
--   key 64  arr(MOD_VALUE_ID, 100000, 6)    -> get_event_par(id, 18) = 100000
--   key 65  set_event_par(id, 19, 100000)   -> arr(MOD_VALUE_ID, 6) = 100000
--   key 66  slot 0 plays a child with 18=9  -> slot 1 reads 9 on it
--   key 67  negative control: set 18=7, ask whether it reads 8 (must be silent)
--
--   tools/oracle-mcp/client.py '{"tool":"oracle_dump_script","arguments":{
--     "nki":"<a .wav>","probe":"tools/kontakt-oracle/probes/event_par_extended.lua"}}'
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

-- Slot 0: every question that is answered on the event the host sent.
local slot0 = table.concat({
  "on init",
  "  declare $a",
  "  declare $ch",
  "end on",
  "on note",
  "  if ($EVENT_NOTE = 60)",
  "    set_event_par($EVENT_ID, 18, 7)",
  "    $a := get_event_par($EVENT_ID, 18)",
  answer("$a = 7"),
  "  end if",
  "  if ($EVENT_NOTE = 61)",
  "    set_event_par($EVENT_ID, 12, 13)",
  "    $a := get_event_par($EVENT_ID, 12)",
  answer("$a = 13"),
  "  end if",
  "  if ($EVENT_NOTE = 62)",
  "    $a := get_event_par($EVENT_ID, 18)",
  answer("$a = 0"),
  "  end if",
  "  if ($EVENT_NOTE = 63)",
  "    set_event_par($EVENT_ID, 19, 5)",
  "    $a := get_event_par($EVENT_ID, 19)",
  answer("$a = 5"),
  "  end if",
  "  if ($EVENT_NOTE = 64)",
  "    set_event_par_arr($EVENT_ID, $EVENT_PAR_MOD_VALUE_ID, 100000, 6)",
  "    $a := get_event_par($EVENT_ID, 18)",
  answer("$a = 100000"),
  "  end if",
  "  if ($EVENT_NOTE = 65)",
  "    set_event_par($EVENT_ID, 19, 100000)",
  "    $a := get_event_par_arr($EVENT_ID, $EVENT_PAR_MOD_VALUE_ID, 6)",
  answer("$a = 100000"),
  "  end if",
  "  if ($EVENT_NOTE = 66)",
  "    $ch := play_note($EVENT_NOTE, $EVENT_VELOCITY, -1, -1)",
  "    set_event_par($ch, 18, 9)",
  "    ignore_event($EVENT_ID)",
  "  end if",
  "  if ($EVENT_NOTE = 67)",
  "    set_event_par($EVENT_ID, 18, 7)",
  "    $a := get_event_par($EVENT_ID, 18)",
  answer("$a = 8"),
  "  end if",
  "end on",
}, "\n") .. "\n"

-- Slot 1: the child key 66's slot 0 played starts here; read its parameter.
local slot1 = table.concat({
  "on init",
  "  declare $b",
  "end on",
  "on note",
  "  if ($EVENT_NOTE = 66 and get_event_par($EVENT_ID, $EVENT_PAR_SOURCE) = 0)",
  "    $b := get_event_par($EVENT_ID, 18)",
  answer("$b = 9"),
  "  end if",
  "end on",
}, "\n") .. "\n"

local path = dir .. "event_par_extended.ksp"
local fh = assert(io.open(path, "w")); fh:write(slot0); fh:write("\n// ---- slot 1 ----\n"); fh:write(slot1); fh:close()
log(string.format('{"ksp":%s,"ksp_path":%s}', jstr(slot0 .. "\n// slot 1\n" .. slot1), jstr(path)))
local ok0, err0 = pcall(kt.set_instrument_script_source, inst, 0, slot0)
log(string.format('{"slot":0,"compile":%s,"err":%s}', tostring(ok0), jstr(err0 or "")))
local ok1, err1 = pcall(kt.set_instrument_script_source, inst, 1, slot1)
log(string.format('{"slot":1,"compile":%s,"err":%s}', tostring(ok1), jstr(err1 or "")))
local saved, serr = pcall(kt.save_instrument, inst, dir .. "event_par_extended.nki", {})
log(string.format('{"saved":%s,"nki":%s,"err":%s}', tostring(saved),
    jstr(dir .. "event_par_extended.nki"), jstr(serr or "")))
log('{"done":true}')
out:close()

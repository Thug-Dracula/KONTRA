-- Slot-liveness probe A: write $ENGINE_PAR_EFFECT_BYPASS into one instrument insert slot at a
-- time, let Kontakt's own engine take the write, and save the instrument so the stored state can
-- be read back. One .nki per case; the cases differ only in which slot / which value was written.
--
--   ORACLE_INPUTS = { "Z:\\...\\Electric Mint.nki" }   ORACLE_OUT = "Z:\\...\\out.jsonl"
--
-- Cases: "addr<k>" sets bypass=1 on KSP slot k with generic $NI_INSERT_BUS (0..7), "g0-<k>" the
-- same with generic 0 (that is $NI_SEND_BUS in KONTRA's model, so it should NOT touch inserts).
-- "control" writes bypass=0 to the three slots of interest (same write path, no state change).
local kt = Kontakt
local out = assert(io.open(ORACLE_OUT, "w"))
local function say(s)
  out:write(s .. "\n")
  out:flush()
end
local DIR = ORACLE_OUT:gsub("[^\\/]+$", "")
local NKI = ORACLE_INPUTS[1]

local CASES = {}
for k = 0, 7 do CASES[#CASES + 1] = { name = "addr" .. k, generic = "1", slot = k, value = 1 } end
for _, k in ipairs({ 1, 3, 5 }) do
  CASES[#CASES + 1] = { name = "g0-" .. k, generic = "0", slot = k, value = 1 }
end
CASES[#CASES + 1] = { name = "control", generic = "1", slot = 1, value = 0, multi = true }

for _, c in ipairs(CASES) do
  kt.reset_multi()
  local idx = kt.load_instrument(NKI)
  if math.type(idx) ~= "integer" then
    say('{"case":"' .. c.name .. '","error":"load: ' .. tostring(idx):gsub('"', "'") .. '"}')
  else
    local src
    if c.multi then
      src = "on init\n"
        .. "  set_engine_par($ENGINE_PAR_EFFECT_BYPASS, 0, -1, 1, $NI_INSERT_BUS)\n"
        .. "  set_engine_par($ENGINE_PAR_EFFECT_BYPASS, 0, -1, 3, $NI_INSERT_BUS)\n"
        .. "  set_engine_par($ENGINE_PAR_EFFECT_BYPASS, 0, -1, 5, $NI_INSERT_BUS)\n"
        .. "end on\n"
    else
      src = "on init\n  set_engine_par($ENGINE_PAR_EFFECT_BYPASS, " .. c.value
        .. ", -1, " .. c.slot .. ", " .. c.generic .. ")\nend on\n"
    end
    pcall(kt.set_instrument_script_name, idx, 3, "probe")
    local ok, err = pcall(kt.set_instrument_script_source, idx, 3, src)
    local t0 = os.time()
    repeat until os.time() - t0 >= 3
    local sok, serr = pcall(kt.save_instrument, idx, DIR .. c.name .. ".nki", {})
    say(string.format('{"case":"%s","slot":%d,"generic":%s,"value":%d,"script":%s,"save":%s}',
      c.name, c.slot, c.generic, c.value, tostring(ok), tostring(sok)))
    if not sok then say('{"save_error":"' .. tostring(serr):gsub('"', "'") .. '"}') end
  end
end
kt.reset_multi()
say('{"done":true}')
out:close()

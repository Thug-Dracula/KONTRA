-- Base-gain probe: bypass the insert slots the liveness record did not ablate (0, 1, 3, 5, 7),
-- and read the instrument's own output volume, which no insert slot can reach.
--
--   ORACLE_INPUTS = { "Z:\\...\\Electric Mint.nki" }   ORACLE_OUT = "Z:\\...\\out.jsonl"
--
-- Same shape as probe_variants.lua: every case writes $ENGINE_PAR_EFFECT_BYPASS through `on init`
-- in KSP script slot 3, differs only in which slots and which values, and is saved so the audio
-- capture can load a file whose only difference is the stored bypass flag.
--   control : 0 0 0        slots 1,3,7 written with 0 - the write path, no state change
--   s0      : slot 0       (empty rack slot: the file must come back identical to control)
--   s1      : slot 1       Eq
--   s3      : slot 3       Hot Solo (the amp)
--   s5      : slot 5       (empty)
--   s7      : slot 7       Convolution
--   s137    : 1 3 7        all three live slots at once
--
-- It also records, per case, Kontakt's own idea of the instrument output volume
-- (Kontakt.get_instrument_volume) so the main/output path has a number that does not depend on
-- any insert slot.
local kt = Kontakt
local out = assert(io.open(ORACLE_OUT, "w"))
local function say(s) out:write(s .. "\n") out:flush() end
local DIR = ORACLE_OUT:gsub("[^\\/]+$", "")
local NKI = ORACLE_INPUTS[1]

local CASES = {
  { name = "control", slots = { 1, 3, 7 }, value = 0 },
  { name = "s0", slots = { 0 }, value = 1 },
  { name = "s1", slots = { 1 }, value = 1 },
  { name = "s3", slots = { 3 }, value = 1 },
  { name = "s5", slots = { 5 }, value = 1 },
  { name = "s7", slots = { 7 }, value = 1 },
  { name = "s137", slots = { 1, 3, 7 }, value = 1 },
}

for _, c in ipairs(CASES) do
  kt.reset_multi()
  local idx = kt.load_instrument(NKI)
  if math.type(idx) ~= "integer" then
    say('{"case":"' .. c.name .. '","error":"' .. tostring(idx):gsub('"', "'") .. '"}')
  else
    local src = "on init\n"
    for _, slot in ipairs(c.slots) do
      src = src .. "  set_engine_par($ENGINE_PAR_EFFECT_BYPASS, " .. c.value
        .. ", -1, " .. slot .. ", $NI_INSERT_BUS)\n"
    end
    src = src .. "end on\n"
    pcall(kt.set_instrument_script_name, idx, 3, "probe")
    local ok, err = pcall(kt.set_instrument_script_source, idx, 3, src)
    local t0 = os.time()
    repeat until os.time() - t0 >= 3
    local vok, vol = pcall(kt.get_instrument_volume, idx)
    local pok, pan = pcall(kt.get_instrument_pan, idx)
    local sok, serr = pcall(kt.save_instrument, idx, DIR .. c.name .. ".nki", {})
    say(string.format(
      '{"case":"%s","slots":[%s],"value":%d,"script":%s,"save":%s,"volume_db":%s,"pan":%s,"script_error":"%s"}',
      c.name, table.concat(c.slots, ","), c.value, tostring(ok), tostring(sok),
      vok and string.format("%.9g", vol) or '"error"',
      pok and string.format("%.9g", pan) or '"error"',
      tostring(err ~= nil and err or ""):gsub('"', "'")))
    if not sok then say('{"save_error":"' .. tostring(serr):gsub('"', "'") .. '"}') end
  end
end
kt.reset_multi()
say('{"done":true}')
out:close()

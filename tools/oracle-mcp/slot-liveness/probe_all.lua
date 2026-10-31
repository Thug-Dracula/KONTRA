-- Base path: bypass every insert slot, so the capture is the voice path alone (no compressor, no
-- diffusion, no chorus, no convolution).
--
--   ORACLE_INPUTS = { "Z:\\...\\Electric Mint.nki" }   ORACLE_OUT = "Z:\\...\\out.jsonl"
--
--   all : slots 0..7 bypassed
--
-- NOTE: this configuration could not be captured — every attempt (20/20) trips the HF-burst
-- screen at +42…+47 dB, because with every slot bypassed the note is 12 dB quieter than `control`
-- and the capture path's burst dominates the 12.9 kHz band. Kept as the record of that limit; use
-- s2/s12 (probe_s2.lua) as the voice-path reference instead.
local kt = Kontakt
local out = assert(io.open(ORACLE_OUT, "w"))
local function say(s) out:write(s .. "\n") out:flush() end
local DIR = ORACLE_OUT:gsub("[^\\/]+$", "")
local NKI = ORACLE_INPUTS[1]

local CASES = {
  { name = "all", slots = { 0, 1, 2, 3, 4, 5, 6, 7 }, value = 1 },
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
    local sok = pcall(kt.save_instrument, idx, DIR .. c.name .. ".nki", {})
    say(string.format('{"case":"%s","slots":[%s],"value":%d,"script":%s,"save":%s,"script_error":"%s"}',
      c.name, table.concat(c.slots, ","), c.value, tostring(ok), tostring(sok),
      tostring(err ~= nil and err or ""):gsub('"', "'")))
  end
end
say('{"done":true}')

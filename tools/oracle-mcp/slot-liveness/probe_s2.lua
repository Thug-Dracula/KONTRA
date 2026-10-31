-- Supercharger ablation: bypass slot 2 (and slot 2 with slot 1), so slot 1's Eq contribution can be
-- measured with the compressor out of the chain.
--
--   ORACLE_INPUTS = { "Z:\\...\\Electric Mint.nki" }   ORACLE_OUT = "Z:\\...\\out.jsonl"
--
--   s2   : slot 2 bypassed (Supercharger out, Eq in)
--   s12  : slots 1 and 2 bypassed (Eq and Supercharger out)
--
-- Same write path as probe_base_gain.lua: `on init` in KSP script slot 3, then save, so the audio
-- capture loads a file whose only difference is the stored bypass flags.
local kt = Kontakt
local out = assert(io.open(ORACLE_OUT, "w"))
local function say(s) out:write(s .. "\n") out:flush() end
local DIR = ORACLE_OUT:gsub("[^\\/]+$", "")
local NKI = ORACLE_INPUTS[1]

local CASES = {
  { name = "s2", slots = { 2 }, value = 1 },
  { name = "s12", slots = { 1, 2 }, value = 1 },
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
    local sok, serr = pcall(kt.save_instrument, idx, DIR .. c.name .. ".nki", {})
    say(string.format('{"case":"%s","slots":[%s],"value":%d,"script":%s,"save":%s,"script_error":"%s"}',
      c.name, table.concat(c.slots, ","), c.value, tostring(ok), tostring(sok),
      tostring(err ~= nil and err or ""):gsub('"', "'")))
  end
end
say('{"done":true}')

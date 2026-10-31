-- Slot-liveness probe B: the four measurement variants, all written by the SAME script shape.
--
--   ORACLE_INPUTS = { "Z:\\...\\Electric Mint.nki" }   ORACLE_OUT = "Z:\\...\\out.jsonl"
--
-- Every case writes $ENGINE_PAR_EFFECT_BYPASS for instrument insert slots 2, 4 and 6 through
-- `on init` in script slot 3, and differs only in those three numbers:
--   control : 0 0 0        (the write path, no state change)
--   sup2    : 1 0 0        Supercharger bypassed  - the positive control (a live slot)
--   chor6   : 0 0 1        Choral (slot 6) bypassed
--   diff4   : 0 1 0        Replika/Diffusion (slot 4) bypassed
--   both46  : 0 1 1        both
local kt = Kontakt
local out = assert(io.open(ORACLE_OUT, "w"))
local function say(s) out:write(s .. "\n") out:flush() end
local DIR = ORACLE_OUT:gsub("[^\\/]+$", "")
local NKI = ORACLE_INPUTS[1]

local CASES = {
  { name = "control", b2 = 0, b4 = 0, b6 = 0 },
  { name = "sup2", b2 = 1, b4 = 0, b6 = 0 },
  { name = "chor6", b2 = 0, b4 = 0, b6 = 1 },
  { name = "diff4", b2 = 0, b4 = 1, b6 = 0 },
  { name = "both46", b2 = 0, b4 = 1, b6 = 1 },
}

for _, c in ipairs(CASES) do
  kt.reset_multi()
  local idx = kt.load_instrument(NKI)
  if math.type(idx) ~= "integer" then
    say('{"case":"' .. c.name .. '","error":"' .. tostring(idx):gsub('"', "'") .. '"}')
  else
    local src = "on init\n"
      .. "  set_engine_par($ENGINE_PAR_EFFECT_BYPASS, " .. c.b2 .. ", -1, 2, $NI_INSERT_BUS)\n"
      .. "  set_engine_par($ENGINE_PAR_EFFECT_BYPASS, " .. c.b4 .. ", -1, 4, $NI_INSERT_BUS)\n"
      .. "  set_engine_par($ENGINE_PAR_EFFECT_BYPASS, " .. c.b6 .. ", -1, 6, $NI_INSERT_BUS)\n"
      .. "end on\n"
    pcall(kt.set_instrument_script_name, idx, 3, "probe")
    local ok, err = pcall(kt.set_instrument_script_source, idx, 3, src)
    local t0 = os.time()
    repeat until os.time() - t0 >= 3
    local sok, serr = pcall(kt.save_instrument, idx, DIR .. c.name .. ".nki", {})
    say(string.format('{"case":"%s","b2":%d,"b4":%d,"b6":%d,"script":%s,"save":%s,"script_error":"%s"}',
      c.name, c.b2, c.b4, c.b6, tostring(ok), tostring(sok), tostring(err ~= nil and err or ""):gsub('"', "'")))
    if not sok then say('{"save_error":"' .. tostring(serr):gsub('"', "'") .. '"}') end
  end
end
kt.reset_multi()
say('{"done":true}')
out:close()

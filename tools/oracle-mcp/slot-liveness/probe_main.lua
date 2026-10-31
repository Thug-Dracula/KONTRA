-- Main/output-path probe: the instrument's own volume, which no insert slot can reach.
--
--   ORACLE_INPUTS = { "Z:\\...\\Electric Mint.nki" }   ORACLE_OUT = "Z:\\...\\out.jsonl"
--
-- Electric Mint has no main-rack slots, so the only gain between the insert rack and the
-- instrument output is the instrument volume in the program header. Each case saves a copy that
-- differs only in that one stored number, so the audio capture can price it the same way it
-- prices an insert slot:
--   volwrite : read the volume back and write the same value - the write path, no state change
--   vol0     : 0 dB   (the stored ratio becomes 1.0)
--   volm6    : -6 dB  (Kontakt's own dB: 6 per doubling)
--   volp6    : +6 dB
local kt = Kontakt
local out = assert(io.open(ORACLE_OUT, "w"))
local function say(s) out:write(s .. "\n") out:flush() end
local DIR = ORACLE_OUT:gsub("[^\\/]+$", "")
local NKI = ORACLE_INPUTS[1]

local CASES = {
  { name = "volwrite", db = nil },
  { name = "vol0", db = 0.0 },
  { name = "volm6", db = -6.0 },
  { name = "volp6", db = 6.0 },
}

for _, c in ipairs(CASES) do
  kt.reset_multi()
  local idx = kt.load_instrument(NKI)
  if math.type(idx) ~= "integer" then
    say('{"case":"' .. c.name .. '","error":"' .. tostring(idx):gsub('"', "'") .. '"}')
  else
    local before = kt.get_instrument_volume(idx)
    local want = c.db
    if want == nil then want = before end
    local ok, err = pcall(kt.set_instrument_volume, idx, want)
    local t0 = os.time()
    repeat until os.time() - t0 >= 3
    local after = kt.get_instrument_volume(idx)
    local sok, serr = pcall(kt.save_instrument, idx, DIR .. c.name .. ".nki", {})
    say(string.format(
      '{"case":"%s","set_db":%.9g,"before_db":%.9g,"after_db":%.9g,"set":%s,"save":%s,"set_error":"%s"}',
      c.name, want, before, after, tostring(ok), tostring(sok),
      tostring(err ~= nil and err or ""):gsub('"', "'")))
    if not sok then say('{"save_error":"' .. tostring(serr):gsub('"', "'") .. '"}') end
  end
end
kt.reset_multi()
say('{"done":true}')
out:close()

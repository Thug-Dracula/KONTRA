-- Does Kontakt 8.14.1 reach the two output channels from a mono sample at the
-- same amplitude as from a stereo one? One instrument, two zones over the SAME
-- 0.5-amplitude source: a mono copy at key 60 and a stereo copy at key 62.
-- Render both keys and compare the peaks: equal peaks is a straight copy, a
-- mono peak of 1/sqrt 2 the stereo one is a constant-power law. Instrument
-- defaults cancel because both zones are in one instrument.
--
--   tools/oracle-mcp/client.py '{"tool":"oracle_dump_script","arguments":{
--     "nki":"/tmp/opencode/mono05.wav","probe":"tools/kontakt-oracle/probes/mono_vs_stereo.lua"}}'
--   (the probe takes the stereo file from $MONO_VS_STEREO_STEREO, else replaces
--    "mono05" with "stereo05" in ORACLE_INPUTS[1])
local kt = Kontakt
local out = assert(io.open(ORACLE_OUT, "w"))
local function log(t) out:write(t .. "\n") out:flush() end
local function jstr(v)
  local s = tostring(v)
  s = s:gsub("\\", "\\\\"):gsub('"', '\\"'):gsub("\n", "\\n"):gsub("\r", "\\r"):gsub("\t", "\\t")
  return '"' .. s .. '"'
end
local dir = ORACLE_OUT:match("^(.*[\\/])") or ""
local monopath = ORACLE_INPUTS[1]
local stereopath = os.getenv("MONO_VS_STEREO_STEREO") or monopath:gsub("mono05", "stereo05")

kt.reset_multi()
local inst = kt.add_instrument()
local zmono = kt.add_zone(inst, 0, monopath)
kt.set_zone_geometry(inst, zmono, { root_key = 60, low_key = 60, high_key = 60 })
local zst = kt.add_zone(inst, 0, stereopath)
kt.set_zone_geometry(inst, zst, { root_key = 62, low_key = 62, high_key = 62 })
log(string.format('{"mono":%s,"stereo":%s}', jstr(monopath), jstr(stereopath)))
local saved, serr = pcall(kt.save_instrument, inst, dir .. "mono_vs_stereo.nki", {})
log(string.format('{"saved":%s,"nki":%s,"err":%s}', tostring(saved),
    jstr(dir .. "mono_vs_stereo.nki"), jstr(serr or "")))
log('{"done":true}')
out:close()

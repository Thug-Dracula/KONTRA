-- Does a Solid G-EQ gain written by a script survive a save/reload, and does
-- Kontakt read it back from the field KONTRA reads? Loads ORACLE_INPUTS[1], reads
-- group 3's HF gain, sets it, saves a copy inside the instrument's own folder (so
-- the library check passes), reloads it, and reads the gain again.
--
--   tools/oracle-mcp/client.py '{"tool":"oracle_dump_script","arguments":{
--     "nki":"<01. Solo Cornet.nki>",
--     "probe":"tools/kontakt-oracle/probes/solid_geq_reload.lua"}}'
local kt = Kontakt
local out = assert(io.open(ORACLE_OUT, "w"))
local function log(t) out:write(t .. "\n") out:flush() end
local function jstr(v)
  local s = tostring(v)
  s = s:gsub("\\", "\\\\"):gsub('"', '\\"'):gsub("\n", "\\n"):gsub("\r", "\\r"):gsub("\t", "\\t")
  return '"' .. s .. '"'
end

local nki = ORACLE_INPUTS[1]
local dir = nki:match("^(.*[\\/])") or ""
local copy = dir .. "..oracle-check-cornet-geq.nki"

kt.reset_multi()
local inst = kt.load_instrument(nki)
if math.type(inst) ~= "integer" then
  log('{"load_error":' .. jstr(inst) .. '}')
  log('{"done":true}')
  out:close()
  return
end
local n = kt.get_num_groups(inst)
local first = n
for _ = 1, 8 do kt.add_group(inst) end

local function read(group, par, g0)
  local text = table.concat({
    "on init", "  declare $v",
    string.format("  $v := get_engine_par($ENGINE_PAR_%s, %d, 0, -1) + 1000", par, group),
    string.format("  set_engine_par($ENGINE_PAR_TUNE, 500000 + 5000 * ($v mod 100), %d, -1, -1)", g0),
    string.format("  set_engine_par($ENGINE_PAR_TUNE, 500000 + 5000 * (($v / 100) mod 100), %d, -1, -1)", g0 + 1),
    string.format("  set_engine_par($ENGINE_PAR_TUNE, 500000 + 5000 * (($v / 10000) mod 100), %d, -1, -1)", g0 + 2),
    "end on",
  }, "\n") .. "\n"
  pcall(kt.set_instrument_script_source, inst, 3, text)
  local t = os.time() repeat until os.time() - t >= 5
  local function normalized(st) return st / 7.2e-5 + 500000 end
  local v = 0
  for d = 0, 2 do
    local tt = normalized(kt.get_group_tune(inst, g0 + d))
    v = v + math.floor((tt - 500000) / 5000 + 0.5) * 100 ^ d
  end
  return v - 1000
end

log(string.format('{"before":"HF_GAIN","value":%d}', read(3, "SEQ_HF_GAIN", first)))
log(string.format('{"before":"LF_GAIN","value":%d}', read(3, "SEQ_LF_GAIN", first)))
local sets = table.concat({
  "on init",
  "  set_engine_par($ENGINE_PAR_SEQ_HF_GAIN, 654321, 3, 0, -1)",
  "  set_engine_par($ENGINE_PAR_SEQ_LF_GAIN, 123456, 3, 0, -1)",
  "end on",
}, "\n") .. "\n"
log('{"set":' .. tostring(pcall(kt.set_instrument_script_source, inst, 3, sets)) .. '}')
local w = os.time() repeat until os.time() - w >= 5
log(string.format('{"after_set":"HF_GAIN","value":%d}', read(3, "SEQ_HF_GAIN", first)))
log(string.format('{"after_set":"LF_GAIN","value":%d}', read(3, "SEQ_LF_GAIN", first)))

local ok, err = pcall(kt.save_instrument, inst, copy, {})
log(string.format('{"saved":%s,"path":%s,"err":%s}', tostring(ok), jstr(copy), jstr(err or "")))
kt.reset_multi()
local again = kt.load_instrument(copy)
if math.type(again) ~= "integer" then
  log('{"reload_error":' .. jstr(again) .. '}')
  log('{"done":true}')
  out:close()
  return
end
inst = again
local m = kt.get_num_groups(inst)
first = m
for _ = 1, 8 do kt.add_group(inst) end
log(string.format('{"reloaded":true,"groups":%d}', m))
log(string.format('{"reloaded":"HF_GAIN","value":%d}', read(3, "SEQ_HF_GAIN", first)))
log(string.format('{"reloaded":"LF_GAIN","value":%d}', read(3, "SEQ_LF_GAIN", first)))
-- The copy is an artifact of the measurement: never leave it in the library.
local removed, rerr = os.remove(copy)
local fh = io.open(copy, "rb")
local gone = fh == nil
if fh then fh:close() end
log(string.format('{"removed":%s,"gone":%s,"err":%s}', tostring(removed ~= nil),
    tostring(gone), jstr(rerr or "")))
log('{"done":true}')
out:close()

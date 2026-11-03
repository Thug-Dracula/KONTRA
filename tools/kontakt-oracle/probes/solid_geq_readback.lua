-- Does `get_engine_par($ENGINE_PAR_SEQ_*_BELL)` answer the Solid G-EQ's stored
-- shape flag, and does `_LMF_GAIN` / `_HF_GAIN` answer the band's gain rather
-- than a field the gain's law couples to it?
--
-- Builds a synthetic instrument with one group, loads the factory INIT Solid G-EQ
-- into that group's insert slot 0, then runs four KSP probes in turn: read the
-- bells, write known gains and bells, read everything back, and return the bells
-- to shelves. Answers are decoded from result groups' tunes (2 base-100 digits).
--
--   tools/oracle-mcp/client.py '{"tool":"oracle_dump_script","arguments":{
--     "probe":"tools/kontakt-oracle/probes/solid_geq_readback.lua"}}'
--
-- Measured (Kontakt 8.14.1, 2026-11-03): see the JSON this writes.
local kt = Kontakt
local out = assert(io.open(ORACLE_OUT, "w"))
local function log(t) out:write(t .. "\n") out:flush() end
local function jstr(v)
  local s = tostring(v)
  s = s:gsub("\\", "\\\\"):gsub('"', '\\"'):gsub("\n", "\\n"):gsub("\r", "\\r"):gsub("\t", "\\t")
  return '"' .. s .. '"'
end
local dir = ORACLE_OUT:match("^(.*[\\/])") or ""

local PRESET = (kt.factory_path or "") .. "\\Presets\\Filters\\Solid G-EQ\\- INIT Solid G-EQ -.nkp"
if not io.open(PRESET, "rb") then
  -- factory_path already ends at the install root on some builds; try both.
  PRESET = (kt.factory_path or "") .. "\\Filters\\Solid G-EQ\\- INIT Solid G-EQ -.nkp"
end

kt.reset_multi()
local inst = kt.add_instrument()
for _ = 1, 39 do kt.add_group(inst) end
local ok, err = pcall(kt.load_fx_preset, PRESET, inst, 0, 0, kt.insert_fx)
log(string.format('{"preset":%s,"load":%s,"err":%s}', jstr(PRESET), tostring(ok),
    jstr(err or "")))
local t0 = os.time() repeat until os.time() - t0 >= 4

local function run(name, lines)
  local src = { "on init", "  declare $v" }
  for _, line in ipairs(lines) do table.insert(src, "  " .. line) end
  table.insert(src, "end on")
  local text = table.concat(src, "\n") .. "\n"
  local path = dir .. "solid_geq_" .. name .. ".ksp"
  local fh = assert(io.open(path, "w")); fh:write(text); fh:close()
  log(string.format('{"run":"%s","ksp":%s,"ksp_path":%s}', name, jstr(text), jstr(path)))
  local okc, errc = pcall(kt.set_instrument_script_source, inst, 0, text)
  log(string.format('{"run":"%s","compile":%s,"err":%s}', name, tostring(okc), jstr(errc or "")))
  local t = os.time() repeat until os.time() - t >= 6
end

local function read(pars, base)
  local lines = {}
  for k, par in ipairs(pars) do
    lines[#lines + 1] = string.format(
      "$v := get_engine_par($ENGINE_PAR_SEQ_%s, 0, 0, -1)", par)
    for d = 0, 2 do
      lines[#lines + 1] = string.format(
        "set_engine_par($ENGINE_PAR_TUNE, 500000 + 5000 * (($v / %d) mod 100), %d, -1, -1)",
        100 ^ d, base + (k - 1) * 3 + d)
    end
  end
  return lines
end

local function decode(pars, base)
  local function normalized(st) return st / 7.2e-5 + 500000 end
  for k, par in ipairs(pars) do
    local value = 0
    for d = 0, 2 do
      local t = normalized(kt.get_group_tune(inst, base + (k - 1) * 3 + d))
      value = value + math.floor((t - 500000) / 5000 + 0.5) * 100 ^ d
    end
    log(string.format('{"par":"%s","value":%d}', par, value))
  end
end

-- 1: the INIT preset's own shapes.
local bells = { "LF_BELL", "HF_BELL" }
run("initial", read(bells, 1))
decode(bells, 1)

-- 2: write, then read back everything.
run("write", {
  "set_engine_par($ENGINE_PAR_SEQ_LF_BELL, 2, 0, 0, -1)",
  "set_engine_par($ENGINE_PAR_SEQ_HF_BELL, 1, 0, 0, -1)",
  "set_engine_par($ENGINE_PAR_SEQ_LF_GAIN, 123456, 0, 0, -1)",
  "set_engine_par($ENGINE_PAR_SEQ_LMF_GAIN, 700000, 0, 0, -1)",
  "set_engine_par($ENGINE_PAR_SEQ_LMF_Q, 100000, 0, 0, -1)",
  "set_engine_par($ENGINE_PAR_SEQ_LMF_FREQ, 300000, 0, 0, -1)",
  "set_engine_par($ENGINE_PAR_SEQ_HF_GAIN, 654321, 0, 0, -1)",
  "set_engine_par($ENGINE_PAR_SEQ_HF_FREQ, 250000, 0, 0, -1)",
})

local all = { "LF_BELL", "HF_BELL", "LF_GAIN", "LMF_GAIN", "LMF_Q", "LMF_FREQ",
              "HF_GAIN", "HF_FREQ" }
run("readback", read(all, 13))
decode(all, 13)

-- 3: a bell switch off returns the shelf.
run("shelf", {
  "set_engine_par($ENGINE_PAR_SEQ_LF_BELL, 0, 0, 0, -1)",
  "set_engine_par($ENGINE_PAR_SEQ_HF_BELL, 0, 0, 0, -1)",
})
run("shelf_read", read(bells, 1))
decode(bells, 1)

-- Leave the instrument in the state measured above and save it, so KONTRA can be
-- asked the same get_engine_par calls about the same file.
run("final", {
  "set_engine_par($ENGINE_PAR_SEQ_LF_BELL, 2, 0, 0, -1)",
  "set_engine_par($ENGINE_PAR_SEQ_HF_BELL, 1, 0, 0, -1)",
  "set_engine_par($ENGINE_PAR_SEQ_LF_GAIN, 123456, 0, 0, -1)",
  "set_engine_par($ENGINE_PAR_SEQ_LMF_GAIN, 700000, 0, 0, -1)",
  "set_engine_par($ENGINE_PAR_SEQ_LMF_Q, 100000, 0, 0, -1)",
  "set_engine_par($ENGINE_PAR_SEQ_LMF_FREQ, 300000, 0, 0, -1)",
  "set_engine_par($ENGINE_PAR_SEQ_HF_GAIN, 654321, 0, 0, -1)",
  "set_engine_par($ENGINE_PAR_SEQ_HF_FREQ, 250000, 0, 0, -1)",
})
local saved, serr = pcall(kt.save_instrument, inst, dir .. "solid_geq_readback.nki", {})
log(string.format('{"saved":%s,"nki":%s,"err":%s}', tostring(saved),
    jstr(dir .. "solid_geq_readback.nki"), jstr(serr or "")))

log('{"done":true}')
out:close()

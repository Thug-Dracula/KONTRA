-- What does Kontakt's `get_engine_par($ENGINE_PAR_SEQ_*)` answer for a real patch's
-- Solid G-EQ inserts? Loads ORACLE_INPUTS[1] (a Brass Band Soloists instrument),
-- finds the groups that hold a Solid G-EQ in insert slot 0, injects one KSP probe
-- into the empty script slot 1, and decodes the answers from appended result
-- groups' tunes (three base-100 digits each, biased by +1000). The patch is never
-- saved.
--
--   tools/oracle-mcp/client.py '{"tool":"oracle_dump_script","arguments":{
--     "nki":"<01. Solo Cornet.nki>",
--     "probe":"tools/kontakt-oracle/probes/solid_geq_patch.lua"}}'
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

kt.reset_multi()
local inst = kt.load_instrument(ORACLE_INPUTS[1])
if math.type(inst) ~= "integer" then
  log('{"load_error":' .. jstr(inst) .. '}')
  log('{"done":true}')
  out:close()
  return
end
local n = kt.get_num_groups(inst)
log(string.format('{"groups":%d,"name":%s}', n, jstr(kt.get_instrument_name(inst))))

-- A handful of groups, so the answer file stays readable.
local GROUPS = math.min(n, 6)
local BIAS = 1000
local pars = { "SEQ_LF_GAIN", "SEQ_LMF_GAIN", "SEQ_HF_GAIN", "SEQ_LF_BELL", "SEQ_HF_BELL" }
local reads = {}
for g = 0, GROUPS - 1 do
  for _, par in ipairs(pars) do reads[#reads + 1] = { g, par } end
end
local first = n
for _ = 1, #reads * 3 do kt.add_group(inst) end

local src = { "on init", "  declare $v" }
for k, r in ipairs(reads) do
  table.insert(src, string.format(
    "  $v := get_engine_par($ENGINE_PAR_%s, %d, 0, -1) + %d", r[2], r[1], BIAS))
  for d = 0, 2 do
    table.insert(src, string.format(
      "  set_engine_par($ENGINE_PAR_TUNE, 500000 + 5000 * (($v / %d) mod 100), %d, -1, -1)",
      100 ^ d, first + (k - 1) * 3 + d))
  end
end
table.insert(src, "end on")
local text = table.concat(src, "\n") .. "\n"
local ksp = dir .. "solid_geq_patch.ksp"
local fh = assert(io.open(ksp, "w")); fh:write(text); fh:close()
log(string.format('{"ksp":%s,"ksp_path":%s}', jstr(text), jstr(ksp)))

local slot = -1
for s = 4, 0, -1 do
  local name = kt.get_instrument_script_name(inst, s)
  if name == nil or name == "" then slot = s end
  if slot >= 0 then break end
end
if slot < 0 then slot = 0 end
local ok, err = pcall(kt.set_instrument_script_source, inst, slot, text)
log(string.format('{"slot":%d,"compile":%s,"err":%s}', slot, tostring(ok), jstr(err or "")))
local t0 = os.time() repeat until os.time() - t0 >= 6

local function normalized(st) return st / 7.2e-5 + 500000 end
for k, r in ipairs(reads) do
  local value = 0
  for d = 0, 2 do
    local t = normalized(kt.get_group_tune(inst, first + (k - 1) * 3 + d))
    value = value + math.floor((t - 500000) / 5000 + 0.5) * 100 ^ d
  end
  log(string.format('{"group":%d,"name":"%s","par":"%s","value":%d}',
      r[1], kt.get_group_name(inst, r[1]), r[2], value - BIAS))
end
log('{"done":true}')
out:close()

-- What does `get_engine_par($ENGINE_PAR_START_CRITERIA_*)` answer for the groups
-- of a real library patch that uses those reads as tags?
--
-- Loads ORACLE_INPUTS[1] (a copy of Steirisches Accordion.nki from Accordions 2),
-- finds the named groups, appends result groups, injects one KSP probe into a free
-- script slot, and decodes the answers from group tunes (two base-100 digits each,
-- biased by +1000). The patch is never saved.
--
--   tools/oracle-mcp/client.py '{"tool":"oracle_dump_script","arguments":{
--     "nki":"<copy of Steirisches Accordion.nki>",
--     "probe":"tools/kontakt-oracle/probes/start_criteria_patch.lua"}}'
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

local want = { "SUS_VEL1_RR1", "SUS_VEL2_RR1", "BUTTON_RR1", "RELEASE_RR1", "KS" }
local found = {}
for g = 0, n - 1 do
  local name = kt.get_group_name(inst, g)
  for _, w in ipairs(want) do
    if name == w and not found[w] then found[w] = g end
  end
end

local BIAS = 1000
local reads = {}
for _, w in ipairs(want) do
  if found[w] then
    for _, par in ipairs({ "MODE", "NEXT_CRIT", "CONTROLLER", "CC_MIN", "CC_MAX" }) do
      reads[#reads + 1] = { w, found[w], par }
    end
  end
end
for _, w in ipairs(want) do
  log(string.format('{"group":%d,"name":"%s"}', found[w] or -1, w))
end

-- Result groups go past the patch's own groups, so no group tune the patch uses
-- is written.
local first = n
for _ = 1, #reads * 2 do kt.add_group(inst) end

local src = { "on init", "  declare $v" }
for k, r in ipairs(reads) do
  table.insert(src, string.format(
    "  $v := get_engine_par($ENGINE_PAR_START_CRITERIA_%s, %d, 0, -1) + %d", r[3], r[2], BIAS))
  for d = 0, 1 do
    table.insert(src, string.format(
      "  set_engine_par($ENGINE_PAR_TUNE, 500000 + 5000 * (($v / %d) mod 100), %d, -1, -1)",
      100 ^ d, first + (k - 1) * 2 + d))
  end
end
table.insert(src, "end on")
local text = table.concat(src, "\n") .. "\n"
local ksp = dir .. "start_criteria_patch.ksp"
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
  for d = 0, 1 do
    local t = normalized(kt.get_group_tune(inst, first + (k - 1) * 2 + d))
    value = value + math.floor((t - 500000) / 5000 + 0.5) * 100 ^ d
  end
  log(string.format('{"group":%d,"name":"%s","par":"%s","value":%d}',
      r[2], r[1], r[3], value - BIAS))
end
log('{"done":true}')
out:close()

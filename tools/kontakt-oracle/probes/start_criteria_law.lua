-- Does `get_engine_par($ENGINE_PAR_START_CRITERIA_*, group, 0, -1)` answer the
-- group's stored start-criteria record, and which field of it?
--
-- Builds a synthetic instrument (no .nki input), sets known start options on four
-- groups, and has a KSP `on init` read eleven names per group. Each answer is
-- written into two group tunes as base-100 digits (500000 + 5000*d, 0.36 st per
-- unit) and decoded here. Answers are biased by +1000 so a negative still decodes.
--
--   tools/oracle-mcp/client.py '{"tool":"oracle_dump_script","arguments":{
--     "probe":"tools/kontakt-oracle/probes/start_criteria_law.lua"}}'
--
-- Measured (Kontakt 8.14.1, 2026-11-03): see the JSON this writes. The instrument
-- is saved beside the output as start_criteria_law.nki so KONTRA's reader can be
-- asked the same question about the same file; delete it afterwards.
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
local inst = kt.add_instrument()
-- Result groups 0..49 (25 reads x 2 digits), then the four groups under test.
for _ = 1, 65 do kt.add_group(inst) end

local CONTROL, TWO_ROWS, KEY_ONLY, EMPTY = 50, 51, 52, 53
local BIAS = 1000
local cases = {
  { CONTROL, "controller row 2/0..64", {
      { mode = "controller", controller = 2, cc_min = 0, cc_max = 64 } } },
  { TWO_ROWS, "key 36..40 and controller 4/10..90", {
      { mode = "key", key_min = 36, key_max = 40, next = "and" },
      { mode = "controller", controller = 4, cc_min = 10, cc_max = 90 } } },
  { KEY_ONLY, "key 48..48", {
      { mode = "key", key_min = 48, key_max = 48 } } },
  { EMPTY, "no rows (always)", nil },
}
for _, case in ipairs(cases) do
  local g, what, rows = case[1], case[2], case[3]
  if rows then
    local ok, err = pcall(kt.set_group_start_options, inst, g, rows)
    log(string.format('{"set_group":%d,"what":%s,"ok":%s,"err":%s}',
        g, jstr(what), tostring(ok), jstr(err or "")))
  end
  local back = kt.get_group_start_options(inst, g)
  local rowsback = {}
  for _, row in ipairs(back or {}) do
    local fields = {}
    for k, v in pairs(row) do fields[#fields + 1] = jstr(k) .. ":" .. jstr(v) end
    table.sort(fields)
    rowsback[#rowsback + 1] = "{" .. table.concat(fields, ",") .. "}"
  end
  log(string.format('{"group":%d,"what":%s,"lua_rows":[%s]}', g, jstr(what),
      table.concat(rowsback, ",")))
end

local reads = {
  { CONTROL, "MODE" }, { CONTROL, "NEXT_CRIT" }, { CONTROL, "KEY_MIN" },
  { CONTROL, "KEY_MAX" }, { CONTROL, "CONTROLLER" }, { CONTROL, "CC_MIN" },
  { CONTROL, "CC_MAX" }, { CONTROL, "CYCLE_CLASS" }, { CONTROL, "ZONE_IDX" },
  { CONTROL, "SLICE_IDX" }, { CONTROL, "SEQ_ONLY" },
  { TWO_ROWS, "MODE" }, { TWO_ROWS, "NEXT_CRIT" }, { TWO_ROWS, "KEY_MIN" },
  { TWO_ROWS, "KEY_MAX" }, { TWO_ROWS, "CONTROLLER" }, { TWO_ROWS, "CC_MIN" },
  { TWO_ROWS, "CC_MAX" },
  { KEY_ONLY, "MODE" }, { KEY_ONLY, "KEY_MIN" }, { KEY_ONLY, "KEY_MAX" },
  { EMPTY, "MODE" }, { EMPTY, "NEXT_CRIT" }, { EMPTY, "KEY_MIN" },
  { EMPTY, "KEY_MAX" }, { EMPTY, "CONTROLLER" }, { EMPTY, "CC_MIN" },
  { EMPTY, "CC_MAX" }, { EMPTY, "CYCLE_CLASS" }, { EMPTY, "ZONE_IDX" },
  { EMPTY, "SLICE_IDX" }, { EMPTY, "SEQ_ONLY" },
}

local src = { "on init", "  declare $v" }
for n, r in ipairs(reads) do
  table.insert(src, string.format(
    "  $v := get_engine_par($ENGINE_PAR_START_CRITERIA_%s, %d, 0, -1) + %d", r[2], r[1], BIAS))
  for d = 0, 1 do
    table.insert(src, string.format(
      "  set_engine_par($ENGINE_PAR_TUNE, 500000 + 5000 * (($v / %d) mod 100), %d, -1, -1)",
      100 ^ d, (n - 1) * 2 + d))
  end
end
table.insert(src, "end on")
local text = table.concat(src, "\n") .. "\n"
local ksp = dir .. "start_criteria_law.ksp"
local fh = assert(io.open(ksp, "w")); fh:write(text); fh:close()
log(string.format('{"ksp":%s,"ksp_path":%s}', jstr(text), jstr(ksp)))

local ok, err = pcall(kt.set_instrument_script_source, inst, 0, text)
log(string.format('{"compile":%s,"err":%s}', tostring(ok), jstr(err or "")))
local t0 = os.time() repeat until os.time() - t0 >= 6

local function normalized(st) return st / 7.2e-5 + 500000 end
for n, r in ipairs(reads) do
  local value = 0
  for d = 0, 1 do
    local t = normalized(kt.get_group_tune(inst, (n - 1) * 2 + d))
    value = value + math.floor((t - 500000) / 5000 + 0.5) * 100 ^ d
  end
  log(string.format('{"group":%d,"par":"%s","value":%d}', r[1], r[2], value - BIAS))
end

local saved, serr = pcall(kt.save_instrument, inst, dir .. "start_criteria_law.nki", {})
log(string.format('{"saved":%s,"err":%s}', tostring(saved), jstr(serr or "")))
log('{"done":true}')
out:close()

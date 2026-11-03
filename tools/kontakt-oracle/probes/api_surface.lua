-- What does this Kontakt's Lua API actually expose? One JSON line: the sorted
-- names of every function on the `Kontakt` table and the version string. Used
-- before calling anything that might not exist.
local kt = Kontakt
local out = assert(io.open(ORACLE_OUT, "w"))
local names = {}
for k, v in pairs(kt) do
  names[#names + 1] = tostring(k) .. ":" .. type(v)
end
table.sort(names)
out:write('{"version":"' .. tostring(kt.version):gsub('"', "'") .. '"}\n')
for i = 1, #names, 40 do
  local chunk = {}
  for j = i, math.min(i + 39, #names) do chunk[#chunk + 1] = names[j] end
  out:write('{"api":["' .. table.concat(chunk, '","') .. '"]}\n')
end
out:write('{"count":' .. #names .. '}\n')
out:write('{"done":true}\n')
out:close()

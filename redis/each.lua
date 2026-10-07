local p = KEYS[1]
local id, source, target = ARGV[1], ARGV[2], ARGV[3]
local ids
if id ~= '*' then ids = {id}
elseif source ~= '*' and target ~= '*' then ids = redis.call('SMEMBERS', p .. 'p:' .. source .. ':' .. target)
elseif source ~= '*' then ids = redis.call('SMEMBERS', p .. 's:' .. source)
elseif target ~= '*' then ids = redis.call('SMEMBERS', p .. 't:' .. target)
else ids = redis.call('HKEYS', p .. 'data') end
local result = {}
for _, found in ipairs(ids) do
    local value = redis.call('HGET', p .. 'data', found)
    if value then
        local s, t = string.match(value, '^(%d+),(%d+)$')
        if (source == '*' or source == s) and (target == '*' or target == t) then
            table.insert(result, tonumber(found))
            table.insert(result, tonumber(s))
            table.insert(result, tonumber(t))
        end
    end
end
return result

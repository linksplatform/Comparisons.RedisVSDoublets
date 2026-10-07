-- One atomic link operation. Each store owns a unique prefix, never a database.
local p = KEYS[1]
local mode = ARGV[1]
local id = ARGV[2]
local source = ARGV[3]
local target = ARGV[4]
local function index_keys(s, t)
    return {p .. 's:' .. s, p .. 't:' .. t, p .. 'p:' .. s .. ':' .. t}
end
local function remove_indexes(value)
    local s, t = string.match(value, '^(%d+),(%d+)$')
    for _, key in ipairs(index_keys(s, t)) do redis.call('SREM', key, id) end
end
if mode == 'create' then
    id = tostring(redis.call('INCR', p .. 'next'))
    source, target = id, id
    redis.call('SADD', p .. 'keys', p .. 'next', p .. 'data')
else
    local old = redis.call('HGET', p .. 'data', id)
    if not old then return redis.error_reply('missing link ' .. id) end
    remove_indexes(old)
end
if mode == 'delete' then
    redis.call('HDEL', p .. 'data', id)
    -- Workloads delete the tail in descending order, matching Doublets reuse.
    local last = tonumber(redis.call('GET', p .. 'next'))
    while last > 0 and redis.call('HEXISTS', p .. 'data', tostring(last)) == 0 do
        last = last - 1
    end
    redis.call('SET', p .. 'next', tostring(last))
else
    redis.call('HSET', p .. 'data', id, source .. ',' .. target)
    for _, key in ipairs(index_keys(source, target)) do
        redis.call('SADD', key, id)
        redis.call('SADD', p .. 'keys', key)
    end
end
return tonumber(id)

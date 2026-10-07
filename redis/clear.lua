local p = KEYS[1]
for _, key in ipairs(redis.call('SMEMBERS', p .. 'keys')) do redis.call('DEL', key) end
redis.call('DEL', p .. 'keys')
return 1

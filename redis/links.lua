#!lua name=links

-- The Redis functions that change links, shared by the C# and the Rust benchmarks.
--
-- A link (id, source, target) is kept in one hash and indexed by three more keys,
-- so every query of the benchmarks is a single native read command:
--
--   links                      hash  id -> "source:target"   Each(id, *, *) = HGET, Each(*, *, *) = HGETALL
--   out:{source}               hash  id -> target            Each(*, source, *) = HGETALL
--   in:{target}                hash  id -> source            Each(*, *, target) = HGETALL
--   pair:{source}:{target}     set   ids                     Each(*, source, target) = SMEMBERS
--
-- A change touches the link and all its indexes, so it runs as a function: atomically,
-- in one round trip, without sending the old source and target back to the client.
-- Redis removes a hash or a set when its last field or member is removed, so an empty
-- database has no keys left. Ids, sources and targets stay strings, so no number is
-- converted to a Lua double.
--
-- The functions read keys that they compute from the link, not keys passed as arguments,
-- so they run on a standalone Redis server only, not in a Redis Cluster.

local function index(id, source, target)
  redis.call('HSET', 'links', id, source .. ':' .. target)
  redis.call('HSET', 'out:' .. source, id, target)
  redis.call('HSET', 'in:' .. target, id, source)
  redis.call('SADD', 'pair:' .. source .. ':' .. target, id)
end

local function unindex(id, link)
  local source, target = string.match(link, '^(%d+):(%d+)$')
  redis.call('HDEL', 'out:' .. source, id)
  redis.call('HDEL', 'in:' .. target, id)
  redis.call('SREM', 'pair:' .. source .. ':' .. target, id)
end

local function existing(id)
  local link = redis.call('HGET', 'links', id)
  if not link then
    return redis.error_reply('link ' .. id .. ' does not exist')
  end
  return link
end

-- FCALL link_create 0 id source target: creates the link, which must not exist yet.
redis.register_function('link_create', function(_, args)
  local id = args[1]
  if redis.call('HEXISTS', 'links', id) == 1 then
    return redis.error_reply('link ' .. id .. ' already exists')
  end
  index(id, args[2], args[3])
  return 1
end)

-- FCALL link_points 0 first last: creates the point links first..=last (id = source = target).
redis.register_function('link_points', function(_, args)
  for id = tonumber(args[1]), tonumber(args[2]) do
    local key = string.format('%d', id)
    index(key, key, key)
  end
  return 1
end)

-- FCALL link_update 0 id source target: sets the source and the target of the link
-- and returns its old "source:target".
redis.register_function('link_update', function(_, args)
  local id = args[1]
  local link = existing(id)
  if type(link) == 'table' then
    return link
  end
  unindex(id, link)
  index(id, args[2], args[3])
  return link
end)

-- FCALL link_delete 0 id: deletes the link and returns its old "source:target".
redis.register_function('link_delete', function(_, args)
  local id = args[1]
  local link = existing(id)
  if type(link) == 'table' then
    return link
  end
  unindex(id, link)
  redis.call('HDEL', 'links', id)
  return link
end)

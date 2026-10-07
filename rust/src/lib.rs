use doublets::{Doublets, Links, data::Flow, mem::Global, unit};
use std::sync::atomic::{AtomicU64, Ordering};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const ANY: usize = usize::MAX;
// doublets 0.5.0 reserves 2^20 elements but loses its first 8192 on growth.
pub const MAX_LINKS: usize = (1 << 20) - 8192 - 1;
pub const OPERATIONS: [&str; 8] = [
    "Create",
    "Update",
    "Delete",
    "Each_All",
    "Each_Identity",
    "Each_Concrete",
    "Each_Outgoing",
    "Each_Incoming",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Link(pub usize, pub usize, pub usize);

pub trait Store {
    fn create(&mut self) -> Result<usize>;
    fn update(&mut self, id: usize, source: usize, target: usize) -> Result<()>;
    fn delete(&mut self, id: usize) -> Result<()>;
    fn each(&mut self, query: [usize; 3], visit: &mut dyn FnMut(Link)) -> Result<()>;
}

pub struct DoubletsStore(unit::Store<usize, Global<unit::LinkPart<usize>>>);
impl DoubletsStore {
    pub fn new() -> Result<Self> {
        Ok(Self(unit::Store::<usize, _>::new(Global::new())?))
    }
}
impl Store for DoubletsStore {
    fn create(&mut self) -> Result<usize> {
        Ok(self.0.create_point()?)
    }
    fn update(&mut self, id: usize, source: usize, target: usize) -> Result<()> {
        self.0.update(id, source, target)?;
        Ok(())
    }
    fn delete(&mut self, id: usize) -> Result<()> {
        self.0.delete(id)?;
        Ok(())
    }
    fn each(&mut self, query: [usize; 3], visit: &mut dyn FnMut(Link)) -> Result<()> {
        let query = query.map(|value| {
            if value == ANY {
                self.0.constants().any
            } else {
                value
            }
        });
        self.0.each_by(query, |link| {
            visit(Link(link.index, link.source, link.target));
            Flow::Continue
        });
        Ok(())
    }
}

pub struct RedisStore {
    connection: redis::Connection,
    prefix: String,
    mutate: redis::Script,
    query: redis::Script,
}
impl RedisStore {
    pub fn new() -> Result<Self> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
        let connection = redis::Client::open(url)?
            .get_connection_with_timeout(std::time::Duration::from_secs(10))?;
        connection.set_read_timeout(Some(std::time::Duration::from_secs(30)))?;
        connection.set_write_timeout(Some(std::time::Duration::from_secs(30)))?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        Ok(Self {
            connection,
            prefix: format!(
                "redis-doublets:{}:{stamp}:{}:",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ),
            mutate: redis::Script::new(include_str!("../../redis/mutate.lua")),
            query: redis::Script::new(include_str!("../../redis/each.lua")),
        })
    }
    pub fn version(&mut self) -> Result<String> {
        let info: String = redis::cmd("INFO")
            .arg("server")
            .query(&mut self.connection)?;
        Ok(info
            .lines()
            .find_map(|line| line.strip_prefix("redis_version:"))
            .ok_or("missing Redis version")?
            .to_string())
    }
    fn mutation(&mut self, mode: &str, id: usize, source: usize, target: usize) -> Result<usize> {
        Ok(self
            .mutate
            .key(&self.prefix)
            .arg(mode)
            .arg(id)
            .arg(source)
            .arg(target)
            .invoke(&mut self.connection)?)
    }
}
impl Store for RedisStore {
    fn create(&mut self) -> Result<usize> {
        self.mutation("create", 0, 0, 0)
    }
    fn update(&mut self, id: usize, source: usize, target: usize) -> Result<()> {
        self.mutation("update", id, source, target)?;
        Ok(())
    }
    fn delete(&mut self, id: usize) -> Result<()> {
        self.mutation("delete", id, 0, 0)?;
        Ok(())
    }
    fn each(&mut self, query: [usize; 3], visit: &mut dyn FnMut(Link)) -> Result<()> {
        let args = query.map(|value| {
            if value == ANY {
                "*".into()
            } else {
                value.to_string()
            }
        });
        let values: Vec<usize> = self
            .query
            .key(&self.prefix)
            .arg(&args)
            .invoke(&mut self.connection)?;
        for link in values.as_chunks::<3>().0 {
            visit(Link(link[0], link[1], link[2]));
        }
        Ok(())
    }
}
impl Drop for RedisStore {
    fn drop(&mut self) {
        if let Err(error) = redis::Script::new(include_str!("../../redis/clear.lua"))
            .key(&self.prefix)
            .invoke::<i32>(&mut self.connection)
        {
            eprintln!("Redis cleanup failed: {error}");
        }
    }
}

pub fn exercise(store: &mut dyn Store, op: usize, background: usize, links: usize) -> Result<()> {
    match op {
        0 => {
            for _ in 0..links {
                std::hint::black_box(store.create()?);
            }
        }
        // Unique asymmetric pairs, with existing endpoints; no merges or cascades.
        1 => {
            for id in 1..=links {
                store.update(id, id, id % background + 1)?;
            }
        }
        2 => {
            for id in (background - links + 1..=background).rev() {
                store.delete(id)?;
            }
        }
        3 => store.each([ANY; 3], &mut |link| {
            std::hint::black_box(link);
        })?,
        4..=7 => {
            for id in 1..=links {
                let query = match op {
                    4 => [id, ANY, ANY],
                    5 => [ANY, id, id],
                    6 => [ANY, id, ANY],
                    _ => [ANY, ANY, id],
                };
                store.each(query, &mut |link| {
                    std::hint::black_box(link);
                })?;
            }
        }
        _ => return Err("unknown operation".into()),
    }
    Ok(())
}
pub fn undo(store: &mut dyn Store, op: usize, background: usize, links: usize) -> Result<()> {
    match op {
        0 => {
            for id in (background + 1..=background + links).rev() {
                store.delete(id)?;
            }
        }
        1 => {
            for id in 1..=links {
                store.update(id, id, id)?;
            }
        }
        2 => {
            for _ in 0..links {
                store.create()?;
            }
        }
        _ => {}
    }
    Ok(())
}

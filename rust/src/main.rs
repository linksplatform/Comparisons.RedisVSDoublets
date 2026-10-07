use redis_vs_doublets::{
    DoubletsStore, MAX_LINKS, OPERATIONS, RedisStore, Result, Store, exercise, undo,
};
use std::time::Instant;

fn setting(name: &str, default: usize) -> Result<usize> {
    Ok(match std::env::var(name) {
        Ok(value) => value.parse()?,
        Err(_) => default,
    })
}
fn main() -> Result<()> {
    let backend = std::env::var("BENCHMARK_BACKEND").unwrap_or_else(|_| "doublets".into());
    let background = setting("BENCHMARK_BACKGROUND_LINKS", 1000)?;
    let links = setting("BENCHMARK_LINKS", 100)?;
    let samples = setting("BENCHMARK_SAMPLES", 10)?;
    if links == 0
        || links > background
        || samples < 3
        || background > MAX_LINKS.saturating_sub(links)
    {
        return Err(
            "require 0 < links <= background, background + links <= 1040383, samples >= 3".into(),
        );
    }
    let mut store: Box<dyn Store> = match backend.as_str() {
        "redis" => {
            let mut store = RedisStore::new()?;
            println!("# redis: {}", store.version()?);
            Box::new(store)
        }
        "doublets" => Box::new(DoubletsStore::new()?),
        _ => return Err("BENCHMARK_BACKEND must be redis or doublets".into()),
    };
    for _ in 0..background {
        store.create()?;
    }
    let variant = if backend == "redis" {
        "Redis"
    } else {
        "Doublets"
    };
    for (op, name) in OPERATIONS.iter().enumerate() {
        // Same bounded warm-up and sampling in both languages. Undo is not timed.
        for _ in 0..3 {
            exercise(&mut *store, op, background, links)?;
            undo(&mut *store, op, background, links)?;
        }
        let mut times = Vec::with_capacity(samples);
        for _ in 0..samples {
            let start = Instant::now();
            exercise(&mut *store, op, background, links)?;
            times.push(start.elapsed().as_nanos() as f64);
            undo(&mut *store, op, background, links)?;
        }
        times.sort_by(f64::total_cmp);
        let median = if samples % 2 == 0 {
            (times[samples / 2 - 1] + times[samples / 2]) / 2.0
        } else {
            times[samples / 2]
        };
        let mean = times.iter().sum::<f64>() / samples as f64;
        let deviation =
            (times.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (samples - 1) as f64).sqrt();
        println!(
            "test {name}/{variant} ... bench: {:.0} ns/iter (+/- {:.0})",
            median.max(1.0),
            deviation
        );
    }
    Ok(())
}

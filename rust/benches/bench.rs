use criterion::Criterion;

mod benchmarks;

/// Runs the benchmarks of both databases, or only of the one named by the
/// `BENCHMARK_BACKEND` environment variable (`redis` or `doublets`), so that
/// they can run in parallel CI jobs and the Doublets job needs no Redis server.
fn main() {
    let backend = std::env::var("BENCHMARK_BACKEND").unwrap_or_default();
    let mut c = Criterion::default().configure_from_args();
    match backend.as_str() {
        "" => {
            benchmarks::redis(&mut c);
            benchmarks::doublets(&mut c);
        }
        "redis" => benchmarks::redis(&mut c),
        "doublets" => benchmarks::doublets(&mut c),
        _ => panic!("BENCHMARK_BACKEND must be `redis` or `doublets`, got `{backend}`"),
    }
    c.final_summary();
}

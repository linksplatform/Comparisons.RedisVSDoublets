using Xunit.Sdk;
using Xunit.v3;

// The Redis tests share (and empty) one database, and the benchmark work adds to the static Benchmarks.Visited,
// so the tests run one at a time.
[assembly: Parallelization(Mode = ParallelMode.None)]

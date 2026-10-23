"""Tests of benchmark_report.py: python3 -m unittest discover -s scripts"""

import contextlib
import io
import tempfile
import unittest
from pathlib import Path

import benchmark_report as report

LINKS, USAGE = report.SUITES


def run(metadata, results):
    return "\n".join(
        [f"# {key}: {value}" for key, value in metadata.items()]
        + [
            f"test {operation}/{implementation} ... bench: {time:,} ns/iter (+/- {deviation:,})"
            for operation, implementation, time, deviation in results
        ]
    )


METADATA = {
    "links": "1000",
    "background links": "10000",
    "cpu": "AMD EPYC 7763 64-Core Processor",
    "date": "2026-10-23",
    "run": "https://github.com/owner/repo/actions/runs/1",
}
DOUBLETS_RUN = run(
    METADATA,
    [
        (operation, implementation, 50_000 + index, 1_000)
        for suite in report.SUITES
        for operation in suite.operations
        for index, (implementation, _, _) in enumerate(suite.doublets)
    ],
)
REDIS_RUN = run(
    METADATA | {"redis": "8.10.2"},
    [
        (operation, implementation, time, 100_000)
        for suite in report.SUITES
        for operation in suite.operations
        for implementation, time in (
            ("Redis_NonTransaction", 80_000_000),
            ("Redis_Transaction", 20_000_000),
            ("Redis_Pipeline", 10_000_000),
        )
    ],
)
BOTH = {"rust-10000-doublets.txt": DOUBLETS_RUN, "rust-10000-redis.txt": REDIS_RUN}


def write_results(directory, files):
    for name, text in files.items():
        (Path(directory) / name).write_text(text, encoding="utf-8")


class Formatting(unittest.TestCase):
    def test_three_significant_digits(self):
        self.assertEqual(report.significant(999.6), "1000")
        self.assertEqual(report.significant(128.4), "128")
        self.assertEqual(report.significant(1.234), "1.23")

    def test_durations_use_readable_units(self):
        self.assertEqual(report.duration(5), "5 ns")
        self.assertEqual(report.duration(79_837), "79.8 µs")
        self.assertEqual(report.duration(10_242_146), "10.2 ms")
        self.assertEqual(report.duration(1_491_658_178), "1.49 s")
        # Rounding to 1000 µs moves to the next unit.
        self.assertEqual(report.duration(999_960), "1 ms")

    def test_large_ratios_have_thousands_separators(self):
        self.assertEqual(report.ratio(36_630.1), "36,600")
        self.assertEqual(report.ratio(128.3), "128")
        self.assertEqual(report.ratio(1.234), "1.23")

    def test_group_names(self):
        self.assertEqual(report.name("Each_All"), "Each All")
        self.assertEqual(report.name("Counter_Increment"), "Counter Increment")


class Comparison(unittest.TestCase):
    def test_faster(self):
        self.assertEqual(report.comparison((79_837, 1_000), (10_242_146, 1_099_253)), "128× faster")

    def test_slower_is_not_reported_as_a_fraction_of_faster(self):
        self.assertEqual(report.comparison((30_000, 100), (10_000, 100)), "3× slower")

    def test_differences_below_the_noise_are_the_same(self):
        self.assertEqual(report.comparison((1_040, 0), (1_000, 0)), "≈ same")

    def test_overlapping_deviations_are_the_same(self):
        self.assertEqual(report.comparison((1_000, 300), (1_500, 300)), "≈ same")

    def test_the_baseline_is_the_fastest_redis_implementation(self):
        times = {
            ("Each_All", "Redis_NonTransaction"): (3_000_000, 0),
            ("Each_All", "Redis_Transaction"): (3_100_000, 0),
            ("Each_All", "Redis_Pipeline"): (3_200_000, 0),
            ("Create", "Redis_NonTransaction"): (20_000_000, 0),
            ("Create", "Redis_Transaction"): (2_000_000, 0),
            ("Create", "Redis_Pipeline"): (1_000_000, 0),
        }
        self.assertEqual(report.baseline(times, "Each_All"), "Redis_NonTransaction")
        self.assertEqual(report.baseline(times, "Create"), "Redis_Pipeline")


class Parsing(unittest.TestCase):
    def test_criterion_bencher_lines_and_metadata(self):
        times, metadata = report.parse(
            "# redis: 8.10.2\n"
            "Benchmarking Create/Redis_Transaction: Warming up for 1.0000 s\n"
            "test Create/Redis_Transaction ... bench:     244,055 ns/iter (+/- 5,345)\n"
            "test Queue_Pop/Doublets_United_Volatile ... bench: 1234 ns/iter (+/- 5)\n"
        )
        self.assertEqual(
            times,
            {
                ("Create", "Redis_Transaction"): (244_055, 5_345),
                ("Queue_Pop", "Doublets_United_Volatile"): (1_234, 5),
            },
        )
        self.assertEqual(metadata, {"redis": "8.10.2"})

    def test_a_failed_benchmark_is_not_published(self):
        with self.assertRaisesRegex(ValueError, "no result"):
            report.parse("test Create/Redis_Transaction ... FAILED\n")

    def test_unknown_benchmarks_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "unknown benchmark"):
            report.parse("test Create/Redis ... bench: 1 ns/iter (+/- 0)\n")
        with self.assertRaisesRegex(ValueError, "unknown benchmark"):
            report.parse("test Rename/Redis_Pipeline ... bench: 1 ns/iter (+/- 0)\n")

    def test_the_split_store_has_no_usage_benchmarks(self):
        with self.assertRaisesRegex(ValueError, "unknown benchmark"):
            report.parse("test Queue_Pop/Doublets_Split_Volatile ... bench: 1 ns/iter (+/- 0)\n")

    def test_a_benchmark_measured_twice_is_rejected(self):
        line = "test Create/Redis_Pipeline ... bench: 1 ns/iter (+/- 0)\n"
        with self.assertRaisesRegex(ValueError, "twice"):
            report.parse(line + line)

    def test_a_zero_time_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "no time"):
            report.parse("test Create/Redis_Pipeline ... bench: 0 ns/iter (+/- 0)\n")


class Loading(unittest.TestCase):
    def test_complete_results(self):
        with tempfile.TemporaryDirectory() as directory:
            write_results(directory, BOTH)
            results = report.load(Path(directory))
        self.assertEqual(list(results), [("rust", 10000)])
        # Links: 2 Doublets and 3 Redis implementations, usage: 1 and 3.
        self.assertEqual(len(results[("rust", 10000)]["times"]), 8 * 5 + 9 * 4)

    def test_results_are_ordered_by_language_and_background_links(self):
        with tempfile.TemporaryDirectory() as directory:
            write_results(
                directory,
                {
                    f"{language}-{background}-{backend}.txt": text
                    for language in ("csharp", "rust")
                    for background in (100000, 10000)
                    for backend, text in (("doublets", DOUBLETS_RUN), ("redis", REDIS_RUN))
                },
            )
            results = report.load(Path(directory))
        self.assertEqual(list(results), [("rust", 10000), ("rust", 100000), ("csharp", 10000), ("csharp", 100000)])

    def test_a_missing_result_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            lines = REDIS_RUN.splitlines()
            write_results(directory, BOTH | {"rust-10000-redis.txt": "\n".join(lines[:-1])})
            with self.assertRaisesRegex(ValueError, "missing results"):
                report.load(Path(directory))

    def test_a_result_of_the_other_backend_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            write_results(directory, BOTH | {"rust-10000-redis.txt": REDIS_RUN + "\n" + DOUBLETS_RUN})
            with self.assertRaisesRegex(ValueError, "unexpected results"):
                report.load(Path(directory))

    def test_a_missing_backend_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            write_results(directory, {"rust-10000-doublets.txt": DOUBLETS_RUN})
            with self.assertRaisesRegex(ValueError, "no redis results"):
                report.load(Path(directory))

    def test_unexpected_file_names_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            write_results(directory, {"10000-doublets.txt": DOUBLETS_RUN})
            with self.assertRaisesRegex(ValueError, "expected"):
                report.load(Path(directory))

    def test_no_results_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory, self.assertRaisesRegex(ValueError, "no results"):
            report.load(Path(directory))


class Markdown(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        write_results(directory.name, BOTH)
        self.results = report.load(Path(directory.name))

    def test_links_table(self):
        lines = report.table(self.results[("rust", 10000)]["times"], LINKS).splitlines()
        self.assertEqual(
            lines[0],
            "| Operation | Doublets United Volatile | Doublets Split Volatile"
            " | Redis NonTransaction | Redis Transaction | Redis Pipeline |",
        )
        self.assertEqual(lines[1], "| --- |" + " ---: |" * 5)
        self.assertEqual(lines[2], "| Create | 50 µs (200× faster) | 50 µs (200× faster) | 80 ms | 20 ms | **10 ms** |")
        self.assertEqual(len(lines), 2 + 8)

    def test_usage_table(self):
        lines = report.table(self.results[("rust", 10000)]["times"], USAGE).splitlines()
        self.assertEqual(
            lines[0],
            "| Operation | Doublets United Volatile | Redis NonTransaction | Redis Transaction | Redis Pipeline |",
        )
        self.assertEqual(lines[2], "| KeyValue Get | 50 µs (200× faster) | 80 ms | 20 ms | **10 ms** |")
        self.assertEqual(len(lines), 2 + 9)

    def test_provenance(self):
        client, library = report.versions("rust")
        self.assertEqual(
            report.provenance("rust", self.results[("rust", 10000)], 1000),
            f"_Median time of one iteration with 1,000 operations. Redis 8.10.2 through {client}; {library}. "
            "CPU AMD EPYC 7763 64-Core Processor, "
            "[GitHub Actions run](https://github.com/owner/repo/actions/runs/1) on 2026-10-23._",
        )

    def test_provenance_of_a_local_run_on_two_machines(self):
        result = self.results[("rust", 10000)]
        result["metadata"]["doublets"] = {"cpu": "Apple M3"}
        result["metadata"]["redis"] = {"cpu": "AMD EPYC 7763 64-Core Processor", "redis": "8.10.2"}
        self.assertIn(
            "CPU AMD EPYC 7763 64-Core Processor (Redis) and Apple M3 (Doublets), a local run._",
            report.provenance("rust", result, 1000),
        )

    def test_versions_come_from_the_repository(self):
        self.assertRegex(report.versions("rust")[0], r"^redis-rs \d+\.\d+\.\d+$")
        self.assertRegex(report.versions("csharp")[1], r"^Platform\.Data\.Doublets \d+\.\d+\.\d+$")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "rust").mkdir()
            (root / "rust" / "Cargo.lock").write_text(
                '[[package]]\nname = "doublets"\nversion = "0.5.0"\n\n[[package]]\nname = "redis"\nversion = "1.8.0"\n'
            )
            self.assertEqual(report.versions("rust", root), ["redis-rs 1.8.0", "doublets 0.5.0"])
            self.assertEqual(
                report.versions("csharp", root),
                ["StackExchange.Redis unknown version", "Platform.Data.Doublets unknown version"],
            )

    def test_section_has_every_language_and_suite(self):
        generated = report.section(self.results, "Docs")
        self.assertIn("### Rust\n\n#### Rust: 10,000 background links, 1,000 operations per iteration\n", generated)
        self.assertIn("### C#\n\n_No results yet._\n", generated)
        self.assertIn("Links storage:\n\n| Operation |", generated)
        self.assertIn("Typical Redis usage:\n\n| Operation |", generated)
        self.assertIn(
            "![Rust, Typical Redis usage, 10,000 background links, 1,000 operations per iteration, log scale]"
            "(Docs/bench_rust_usage_log_scale_10000.png)",
            generated,
        )
        self.assertTrue(generated.startswith(report.LINT_OFF))
        self.assertTrue(generated.endswith(report.LINT_ON))

    def test_replacing_the_section_twice_gives_the_same_document(self):
        document = f"# Title\n\n{report.START_MARKER}\nold\n{report.END_MARKER}\n\n## Limitations\n"
        once = report.replace_section(document, report.section(self.results, "Docs"))
        self.assertEqual(report.replace_section(once, report.section(self.results, "Docs")), once)
        self.assertNotIn("old", once)
        self.assertTrue(once.startswith("# Title\n\n") and once.endswith("\n\n## Limitations\n"))

    def test_missing_markers_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "markers"):
            report.replace_section("# Title\n", "generated")

    def test_different_links_per_iteration_are_rejected(self):
        result = self.results[("rust", 10000)]
        result["metadata"]["redis"]["links"] = "100"
        with self.assertRaisesRegex(ValueError, "different or unknown links"):
            report.links_per_iteration(result)


class Charts(unittest.TestCase):
    def test_linear_and_log_charts_are_written(self):
        try:
            import matplotlib  # noqa: F401
        except ImportError:
            self.skipTest("matplotlib is not installed")
        with tempfile.TemporaryDirectory() as directory:
            write_results(directory, BOTH)
            written = report.charts(report.load(Path(directory)), Path(directory) / "Docs")
            self.assertEqual(
                [path.name for path in written],
                [
                    "bench_rust_links_10000.png",
                    "bench_rust_links_log_scale_10000.png",
                    "bench_rust_usage_10000.png",
                    "bench_rust_usage_log_scale_10000.png",
                ],
            )
            for path in written:
                self.assertEqual(path.read_bytes()[:8], b"\x89PNG\r\n\x1a\n")


class CommandLine(unittest.TestCase):
    def test_the_readme_is_updated(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "results").mkdir()
            write_results(root / "results", BOTH)
            readme = root / "README.md"
            readme.write_text(f"# Title\n\n{report.START_MARKER}\n{report.END_MARKER}\n", encoding="utf-8")
            with contextlib.redirect_stdout(io.StringIO()) as output:
                report.main([str(root / "results"), "--readme", str(readme)])
            self.assertIn("### Rust", output.getvalue())
            self.assertIn("| Set Contains |", readme.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()

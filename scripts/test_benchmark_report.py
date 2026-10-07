import tempfile
import unittest
from pathlib import Path

import benchmark_report as report


def output(backend, links=5, background=20):
    metadata = f"# links: {links}\n# background: {background}\n# samples: 10\n# cpu: Test CPU\n# date: 2026-10-06\n# runtime: test runtime\n# commit: abc123\n"
    if backend == "redis":
        metadata += "# redis: 7.4.2\n"
    variant = "Redis" if backend == "redis" else "Doublets"
    return metadata + "\n".join(
        f"test {operation.replace(' ', '_')}/{variant} ... bench: 100 ns/iter (+/- 1)"
        for operation in report.OPERATIONS
    )


class ReportTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.path = Path(self.temporary.name)
        for language in ("rust", "csharp"):
            for backend in ("redis", "doublets"):
                (self.path / f"{language}-20-{backend}.txt").write_text(output(backend))

    def test_complete_results(self):
        results = report.load(self.path)
        self.assertEqual(len(results), 2)
        generated = report.section(results, "docs")
        for expected in (
            "### Rust",
            "### C#",
            "≈ same",
            "7.4.2",
            "Test CPU",
            "test runtime",
            "abc123",
            "linear scale",
            "log scale",
        ):
            self.assertIn(expected, generated)

    def test_comparisons(self):
        self.assertEqual(report.comparison((100, 0), (1000, 0)), "10× faster")
        self.assertEqual(report.comparison((1000, 0), (100, 0)), "10× slower")
        self.assertEqual(report.comparison((100, 0), (104, 0)), "≈ same")
        self.assertEqual(report.comparison((100, 100), (190, 100)), "≈ same")

    def test_missing_backend(self):
        (self.path / "rust-20-redis.txt").unlink()
        with self.assertRaisesRegex(ValueError, "no redis results"):
            report.load(self.path)

    def test_missing_language(self):
        for file in self.path.glob("csharp-*"):
            file.unlink()
        with self.assertRaisesRegex(ValueError, "same background sizes"):
            report.load(self.path)

    def test_invalid_files(self):
        (self.path / "garbage.txt").write_text("oops")
        with self.assertRaisesRegex(ValueError, "expected"):
            report.load(self.path)

    def test_missing_or_unexpected_operation(self):
        file = self.path / "rust-20-redis.txt"
        for text in ("", output("doublets")):
            file.write_text(text)
            with self.assertRaisesRegex(ValueError, "missing results"):
                report.load(self.path)

    def test_mismatched_workload(self):
        file = self.path / "rust-20-redis.txt"
        for key, old, new in (
            ("links", "5", "4"),
            ("samples", "10", "3"),
            ("background", "20", "21"),
        ):
            file.write_text(
                output("redis").replace(f"# {key}: {old}", f"# {key}: {new}")
            )
            with self.assertRaises(ValueError):
                report.load(self.path)

    def test_languages_and_commits_match(self):
        for path, old, new in (
            ("csharp-20-redis.txt", "# links: 5", "# links: 4"),
            ("rust-20-redis.txt", "# commit: abc123", "# commit: def456"),
        ):
            file = self.path / path
            file.write_text(output("redis").replace(old, new))
            with self.assertRaises(ValueError):
                report.load(self.path)
            file.write_text(output("redis"))

    def test_missing_provenance(self):
        file = self.path / "rust-20-redis.txt"
        for key in ("cpu", "date", "runtime", "commit", "redis"):
            file.write_text(
                "\n".join(
                    line
                    for line in output("redis").splitlines()
                    if not line.startswith(f"# {key}:")
                )
            )
            with self.assertRaisesRegex(ValueError, "missing"):
                report.load(self.path)

    def test_parse_rejects_invalid_lines(self):
        for line in (
            "test Create/Redis ... FAILED",
            "test Unknown/Redis ... bench: 1 ns/iter (+/- 0)",
            "test Create/Redis ... bench: 0 ns/iter (+/- 0)",
        ):
            with self.assertRaises(ValueError):
                report.parse(line)

    def test_duplicate_result(self):
        line = "test Create/Redis ... bench: 1 ns/iter (+/- 0)"
        with self.assertRaisesRegex(ValueError, "duplicate"):
            report.parse(line + "\n" + line)

    def test_thousands(self):
        times, _ = report.parse("test Create/Redis ... bench: 1,000 ns/iter (+/- 100)")
        self.assertEqual(times[("Create", "Redis")], (1000, 100))
        self.assertEqual(report.duration(1000000), "1 ms")

    def test_markers_preserve_authored_text(self):
        document = f"before\n{report.START_MARKER}\nold\n{report.END_MARKER}\nafter"
        self.assertEqual(
            report.replace_section(document, "new"), document.replace("old", "new")
        )
        with self.assertRaises(ValueError):
            report.replace_section("missing", "new")

    def test_duplicate_markers(self):
        document = report.START_MARKER + report.END_MARKER
        with self.assertRaises(ValueError):
            report.replace_section(document + document, "new")

    def test_empty_directory(self):
        with tempfile.TemporaryDirectory() as empty:
            with self.assertRaisesRegex(ValueError, "no results"):
                report.load(empty)

    def test_charts_and_readme_end_to_end(self):
        readme = self.path / "README.md"
        readme.write_text(
            f"prefix\n{report.START_MARKER}\nold\n{report.END_MARKER}\nsuffix"
        )
        report.main(
            [
                str(self.path),
                "--readme",
                str(readme),
                "--charts",
                str(self.path / "docs"),
            ]
        )
        self.assertEqual(len(list((self.path / "docs").glob("*.png"))), 4)
        for image in (self.path / "docs").glob("*.png"):
            self.assertTrue(image.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"))
        self.assertTrue(readme.read_text().startswith("prefix\n"))
        self.assertTrue(readme.read_text().endswith("\nsuffix"))


if __name__ == "__main__":
    unittest.main()

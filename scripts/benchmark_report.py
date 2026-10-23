#!/usr/bin/env python3
"""Turns the benchmark output into the README results and charts.

Input: `RESULTS_DIR/<language>-<background links>-<backend>.txt`, the output of
one benchmark run for one language (`rust` or `csharp`), one number of
background links and one backend (`redis` or `doublets`). Both languages print
the bencher format of Criterion,

    test Create/Redis_Transaction ... bench:     244,055 ns/iter (+/- 5,345)

with the median time of one iteration and its standard deviation, after
`# key: value` lines that describe the run (see `scripts/benchmark_header.py`).

Every run measures two suites: the links storage operations and the typical
Redis usage reimplemented in links. Output, for every language, number of
background links and suite:
- a Markdown table, written into the README between the markers;
- `<charts>/bench_<language>_<suite>_<background links>.png` (linear scale) and
  `<charts>/bench_<language>_<suite>_log_scale_<background links>.png` (log scale).

Usage: benchmark_report.py RESULTS_DIR [--readme README.md] [--charts docs]
"""

from __future__ import annotations

import argparse
import logging
import os
import re
from dataclasses import dataclass
from pathlib import Path

# Enable detailed tracing. Set to True to see every parsed result.
DEBUG = False
logging.basicConfig(level=logging.INFO if DEBUG else logging.WARNING, format="%(message)s")
log = logging.getLogger("benchmark_report")

ROOT = Path(__file__).resolve().parent.parent
START_MARKER = "<!-- results:start -->"
END_MARKER = "<!-- results:end -->"
# The wide tables cannot be wrapped, and every language repeats the same headings.
LINT_OFF = "<!-- markdownlint-disable MD013 MD024 -->"
LINT_ON = "<!-- markdownlint-restore -->"

# Medians that differ by less than 5%, or whose ranges of one standard
# deviation overlap, are not called a difference.
NOISE = 0.05

# Benchmark id, column name and chart color of every implementation.
Implementation = tuple[str, str, str]
DOUBLETS_UNITED: Implementation = ("Doublets_United_Volatile", "Doublets United Volatile", "salmon")
DOUBLETS_SPLIT: Implementation = ("Doublets_Split_Volatile", "Doublets Split Volatile", "lightgreen")
REDIS: list[Implementation] = [
    ("Redis_NonTransaction", "Redis NonTransaction", "lightblue"),
    ("Redis_Transaction", "Redis Transaction", "blue"),
    ("Redis_Pipeline", "Redis Pipeline", "navy"),
]
REDIS_IDS = {implementation for implementation, _, _ in REDIS}


@dataclass(frozen=True)
class Suite:
    """One kind of benchmarks: its operations (benchmark groups) and implementations."""

    key: str
    title: str
    operations: list[str]
    doublets: list[Implementation]

    @property
    def implementations(self) -> list[Implementation]:
        return self.doublets + REDIS

    def backend(self, backend: str) -> list[Implementation]:
        return REDIS if backend == "redis" else self.doublets


SUITES = [
    Suite(
        "links",
        "Links storage",
        [
            "Create",
            "Update",
            "Delete",
            "Each_All",
            "Each_Identity",
            "Each_Concrete",
            "Each_Outgoing",
            "Each_Incoming",
        ],
        [DOUBLETS_UNITED, DOUBLETS_SPLIT],
    ),
    # The split store of doublets 0.5.0 loses links with external references
    # (see experiments/README.md), so the usage benchmarks use the united store.
    Suite(
        "usage",
        "Typical Redis usage",
        [
            "KeyValue_Get",
            "KeyValue_Set",
            "KeyValue_Delete",
            "Counter_Increment",
            "Queue_Push",
            "Queue_Pop",
            "Set_Add",
            "Set_Remove",
            "Set_Contains",
        ],
        [DOUBLETS_UNITED],
    ),
]
SUITE_OF = {operation: suite for suite in SUITES for operation in suite.operations}
BACKENDS = ["redis", "doublets"]

# Name in the README, and how to find the versions of the Redis client and of
# the Doublets library in the repository: (name, file, regex with the version).
LANGUAGES = {
    "rust": (
        "Rust",
        [
            ("redis-rs", "rust/Cargo.lock", r'name = "redis"\nversion = "([^"]+)"'),
            ("doublets", "rust/Cargo.lock", r'name = "doublets"\nversion = "([^"]+)"'),
        ],
    ),
    "csharp": (
        "C#",
        [
            (
                "StackExchange.Redis",
                "csharp/RedisVSDoublets/RedisVSDoublets.csproj",
                r'Include="StackExchange\.Redis" Version="([^"]+)"',
            ),
            (
                "Platform.Data.Doublets",
                "csharp/RedisVSDoublets/RedisVSDoublets.csproj",
                r'Include="Platform\.Data\.Doublets" Version="([^"]+)"',
            ),
        ],
    ),
}

# For example: `test Create/Redis_Transaction ... bench:     244,055 ns/iter (+/- 5,345)`
# (Criterion 0.8 separates thousands with commas, older versions did not).
BENCH_LINE = re.compile(r"test\s+(\w+)/(\w+)\s+\.\.\.\s+bench:\s+([\d,]+)\s+ns/iter\s+\(\+/-\s+([\d,]+)\)")
METADATA_LINE = re.compile(r"#\s*([\w ]+?)\s*:\s*(.*?)\s*$")
FILE_NAME = re.compile(r"(\w+)-(\d+)-(\w+)")

Times = dict[tuple[str, str], tuple[int, int]]
Result = dict[str, dict]


def name(operation: str) -> str:
    """The name of a benchmark group in the tables: `Each_All` is `Each All`."""
    return operation.replace("_", " ")


def parse(text: str, path: object = "<text>") -> tuple[Times, dict[str, str]]:
    """Returns ({(operation, implementation): (median ns, std dev ns)}, {key: value})."""
    times: Times = {}
    metadata: dict[str, str] = {}
    for line in text.splitlines():
        if line.startswith("#"):
            match = METADATA_LINE.match(line)
            if match:
                metadata[match.group(1).lower()] = match.group(2)
            continue
        if not line.startswith("test "):
            continue
        # A line without a time means the benchmark reported an error
        # instead of its result; such a run must not be published.
        match = BENCH_LINE.match(line)
        if not match:
            raise ValueError(f"{path}: no result in line: {line}")
        operation, implementation, median_text, deviation_text = match.groups()
        suite = SUITE_OF.get(operation)
        if suite is None or implementation not in {known for known, _, _ in suite.implementations}:
            raise ValueError(f"{path}: unknown benchmark {operation}/{implementation}")
        if (operation, implementation) in times:
            raise ValueError(f"{path}: {operation}/{implementation} is measured twice")
        median, deviation = int(median_text.replace(",", "")), int(deviation_text.replace(",", ""))
        if median <= 0:
            raise ValueError(f"{path}: {operation}/{implementation} took no time")
        times[(operation, implementation)] = (median, deviation)
        log.info("%s: %s %s: %s ns (+/- %s)", path, operation, implementation, median, deviation)
    return times, metadata


def expected(backend: str) -> set[tuple[str, str]]:
    """Every benchmark that one run of `backend` measures."""
    return {
        (operation, implementation)
        for suite in SUITES
        for operation in suite.operations
        for implementation, _, _ in suite.backend(backend)
    }


def load(directory: Path) -> dict[tuple[str, int], Result]:
    """All results in `directory`: {(language, background): {"times", "metadata"}}."""
    results: dict[tuple[str, int], Result] = {}
    for path in sorted(Path(directory).glob("*.txt")):
        match = FILE_NAME.fullmatch(path.stem)
        if not match or match.group(1) not in LANGUAGES or match.group(3) not in BACKENDS:
            raise ValueError(f"{path}: expected <{'|'.join(LANGUAGES)}>-<links>-<{'|'.join(BACKENDS)}>.txt")
        language, background, backend = match.group(1), int(match.group(2)), match.group(3)
        times, metadata = parse(path.read_text(encoding="utf-8"), path)
        if set(times) != expected(backend):
            missing = sorted(expected(backend) - set(times))
            unexpected = sorted(set(times) - expected(backend))
            raise ValueError(f"{path}: missing results {missing}, unexpected results {unexpected}")
        result = results.setdefault((language, background), {"times": {}, "metadata": {}})
        result["times"].update(times)
        result["metadata"][backend] = metadata
    if not results:
        raise ValueError(f"no results found in {directory}/")
    for (language, background), result in results.items():
        missing_backends = set(BACKENDS) - set(result["metadata"])
        if missing_backends:
            raise ValueError(
                f"{language}, {background} background links: no {', '.join(sorted(missing_backends))} results"
            )
    return dict(sorted(results.items(), key=lambda item: (list(LANGUAGES).index(item[0][0]), item[0][1])))


def significant(value: float) -> str:
    """Three significant digits without an exponent: 999.6 is 1000, not 1e+03."""
    return f"{float(f'{value:.3g}'):g}"


def duration(nanoseconds: float) -> str:
    rounded = float(significant(nanoseconds))
    for unit, scale in (("s", 1e9), ("ms", 1e6), ("µs", 1e3)):
        if rounded >= scale:
            return f"{significant(rounded / scale)} {unit}"
    return f"{significant(rounded)} ns"


def ratio(value: float) -> str:
    """`significant`, with thousands separators: 36,600, not 36600."""
    rounded = float(significant(value))
    return f"{rounded:,.0f}" if rounded >= 1000 else significant(rounded)


def comparison(measured: tuple[int, int], reference: tuple[int, int]) -> str:
    """How `measured` compares with `reference`, both (median, std dev)."""
    (median, deviation), (reference_median, reference_deviation) = measured, reference
    overlapping = (
        median - deviation <= reference_median + reference_deviation
        and reference_median - reference_deviation <= median + deviation
    )
    if overlapping or max(median, reference_median) / min(median, reference_median) < 1 + NOISE:
        return "≈ same"
    if median < reference_median:
        return f"{ratio(reference_median / median)}× faster"
    return f"{ratio(median / reference_median)}× slower"


def baseline(times: Times, operation: str) -> str:
    """The fastest Redis implementation of `operation`."""
    return min(
        (implementation for implementation, _, _ in REDIS),
        key=lambda implementation: times[(operation, implementation)][0],
    )


def table(times: Times, suite: Suite) -> str:
    header = "| Operation | " + " | ".join(column for _, column, _ in suite.implementations) + " |"
    separator = "| --- |" + " ---: |" * len(suite.implementations)
    rows = [header, separator]
    for operation in suite.operations:
        fastest = baseline(times, operation)
        cells = []
        for implementation, _, _ in suite.implementations:
            measured = times[(operation, implementation)]
            if implementation == fastest:
                cells.append(f"**{duration(measured[0])}**")
            elif implementation in REDIS_IDS:
                cells.append(duration(measured[0]))
            else:
                cells.append(f"{duration(measured[0])} ({comparison(measured, times[(operation, fastest)])})")
        rows.append(f"| {name(operation)} | " + " | ".join(cells) + " |")
    return "\n".join(rows)


def versions(language: str, root: Path = ROOT) -> list[str]:
    """`name version` of the Redis client and the Doublets library of `language`."""
    found = []
    for library, file, pattern in LANGUAGES[language][1]:
        path = root / file
        match = re.search(pattern, path.read_text(encoding="utf-8")) if path.exists() else None
        found.append(f"{library} {match.group(1) if match else 'unknown version'}")
    return found


def provenance(language: str, result: Result, links: int, root: Path = ROOT) -> str:
    """Where the numbers come from: versions, machines and the run."""
    redis, doublets = result["metadata"]["redis"], result["metadata"]["doublets"]
    client, library = versions(language, root)
    cpus = {redis.get("cpu"), doublets.get("cpu")} - {None}
    if not cpus:
        machine = "CPU unknown"
    elif len(cpus) == 1:
        machine = f"CPU {cpus.pop()}"
    else:
        machine = f"CPU {redis.get('cpu', 'unknown')} (Redis) and {doublets.get('cpu', 'unknown')} (Doublets)"
    run, date = redis.get("run") or doublets.get("run"), redis.get("date") or doublets.get("date")
    source = f"[GitHub Actions run]({run})" if run else "a local run"
    if date:
        source += f" on {date}"
    runtimes = " and ".join(sorted({redis.get("runtime", "unknown"), doublets.get("runtime", "unknown")}))
    commits = " and ".join(sorted({redis.get("commit", "unknown")[:7], doublets.get("commit", "unknown")[:7]}))
    return (
        f"_Median time of one iteration with {links:,} operations. "
        f"Redis {redis.get('redis', 'unknown version')} through {client}; {library}. "
        f"{machine}, runtime {runtimes}, commit {commits}, {source}._"
    )


def links_per_iteration(result: Result) -> int:
    counts = {metadata.get("links") for metadata in result["metadata"].values()}
    if len(counts) != 1 or None in counts:
        raise ValueError(f"the backends were measured with different or unknown links per iteration: {counts}")
    return int(counts.pop())


def chart_names(language: str, suite: Suite, background: int) -> tuple[str, str]:
    prefix = f"bench_{language}_{suite.key}"
    return f"{prefix}_{background}.png", f"{prefix}_log_scale_{background}.png"


def section(results: dict[tuple[str, int], Result], charts_link: str | None) -> str:
    """The Markdown between the markers: a heading per language, number of background links and suite."""
    lines = [LINT_OFF, ""]
    for language, (title, _) in LANGUAGES.items():
        lines += [f"### {title}", ""]
        backgrounds = [background for (lang, background) in results if lang == language]
        if not backgrounds:
            lines += ["_No results yet._", ""]
            continue
        for background in backgrounds:
            result = results[(language, background)]
            links = links_per_iteration(result)
            heading = f"{background:,} background links, {links:,} operations per iteration"
            lines += [f"#### {title}: {heading}", "", provenance(language, result, links), ""]
            for suite in SUITES:
                lines += [f"{suite.title}:", "", table(result["times"], suite), ""]
                if charts_link is not None:
                    linear, log_scale = chart_names(language, suite, background)
                    lines += [
                        f"![{title}, {suite.title}, {heading}, linear scale]({charts_link}/{linear})",
                        f"![{title}, {suite.title}, {heading}, log scale]({charts_link}/{log_scale})",
                        "",
                    ]
    lines += [LINT_ON]
    return "\n".join(lines)


def replace_section(document: str, generated: str) -> str:
    pattern = re.compile(re.escape(START_MARKER) + ".*?" + re.escape(END_MARKER), re.DOTALL)
    if not pattern.search(document):
        raise ValueError(f"{START_MARKER} ... {END_MARKER} markers are missing")
    return pattern.sub(lambda _: f"{START_MARKER}\n{generated}\n{END_MARKER}", document)


def chart(times: Times, suite: Suite, title: str, path: Path, log_scale: bool) -> None:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from matplotlib.ticker import EngFormatter

    implementations = suite.implementations
    positions, width = range(len(suite.operations)), 0.8 / len(implementations)
    figure, axis = plt.subplots(figsize=(12, 8))
    # Seconds, so that the axis shows `10 ms` instead of `1e7`.
    values = [
        [times[(operation, implementation)][0] / 1e9 for operation in suite.operations]
        for implementation, _, _ in implementations
    ]
    if not log_scale:
        # Bars of 0.5% of the longest bar stay visible (about 4 pixels).
        min_visible = max(max(series) for series in values) * 0.005
        values = [[max(value, min_visible) for value in series] for series in values]
    for index, ((_, label, color), series) in enumerate(zip(implementations, values, strict=True)):
        offset = (index - (len(implementations) - 1) / 2) * width
        axis.barh([position + offset for position in positions], series, width, label=label, color=color)
    axis.set_xlabel("Median time of one iteration" + (", log scale" if log_scale else ""))
    if log_scale:
        axis.set_xscale("log")
    axis.xaxis.set_major_formatter(EngFormatter(unit="s"))
    axis.grid(axis="x", alpha=0.3)
    axis.set_title(title)
    axis.set_yticks(list(positions))
    axis.set_yticklabels([name(operation) for operation in suite.operations])
    axis.invert_yaxis()
    axis.legend(loc="upper left", bbox_to_anchor=(1.01, 1))
    figure.tight_layout()
    figure.savefig(path)
    plt.close(figure)
    log.info("%s saved", path)


def charts(results: dict[tuple[str, int], Result], directory: Path) -> list[Path]:
    directory.mkdir(parents=True, exist_ok=True)
    written = []
    for (language, background), result in results.items():
        for suite in SUITES:
            title = (
                f"{LANGUAGES[language][0]}, {suite.title}: {background:,} background links, "
                f"{links_per_iteration(result):,} operations per iteration"
            )
            linear, log_scale = chart_names(language, suite, background)
            chart(result["times"], suite, title, directory / linear, log_scale=False)
            chart(result["times"], suite, title, directory / log_scale, log_scale=True)
            written += [directory / linear, directory / log_scale]
    return written


def main(arguments: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("results", type=Path, help="directory with the benchmark output")
    parser.add_argument("--readme", type=Path, help="README to update in place")
    parser.add_argument("--charts", type=Path, help="directory for the charts")
    options = parser.parse_args(arguments)
    results = load(options.results)
    link = None
    if options.charts is not None:
        charts(results, options.charts)
        base = options.readme.resolve().parent if options.readme else Path.cwd()
        link = Path(os.path.relpath(options.charts.resolve(), base)).as_posix()
    generated = section(results, link)
    print(generated)
    if options.readme is not None:
        document = options.readme.read_text(encoding="utf-8")
        options.readme.write_text(replace_section(document, generated), encoding="utf-8")


if __name__ == "__main__":
    main()

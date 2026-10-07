#!/usr/bin/env python3
"""Turns the benchmark output into the README results and charts.

Input: `RESULTS_DIR/<language>-<background links>-<backend>.txt`, the output of
one benchmark run for one language (`rust` or `csharp`), one number of
background links and one backend (`redis` or `doublets`). Both languages print
a shared bencher-compatible format,

    test Create/Redis ... bench:  44,055,505 ns/iter (+/- 5,345,991)

with the median time of one iteration and its standard deviation, after
`# key: value` lines that describe the run (see `scripts/benchmark_header.sh`).

Output, for every language and number of background links:
- a Markdown table, written into the README between the markers;
- `<charts>/bench_<language>_<background links>.png` (linear scale) and
  `<charts>/bench_<language>_log_scale_<background links>.png` (log scale).

Usage: benchmark_report.py RESULTS_DIR [--readme README.md] [--charts Docs]
"""

import argparse
import logging
import os
import re
from pathlib import Path

# Enable detailed tracing. Set to True to see every parsed result.
DEBUG = False
logging.basicConfig(
    level=logging.INFO if DEBUG else logging.WARNING, format="%(message)s"
)
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

OPERATIONS = [
    "Create",
    "Update",
    "Delete",
    "Each All",
    "Each Identity",
    "Each Concrete",
    "Each Outgoing",
    "Each Incoming",
]

# Benchmark id, column name and chart color of every implementation.
DOUBLETS = [("Doublets", "Doublets (in memory)", "seagreen")]
REDIS = [("Redis", "Redis", "steelblue")]
IMPLEMENTATIONS = REDIS + DOUBLETS
IMPLEMENTATION_IDS = {implementation for implementation, _, _ in IMPLEMENTATIONS}
DOUBLETS_IDS = {implementation for implementation, _, _ in DOUBLETS}
BACKENDS = {"doublets": DOUBLETS, "redis": REDIS}
NOT_MEASURED = set()

# Name in the README, and how to find the versions of the Redis driver and of
# the Doublets library in the repository: (file, regex with the version).
LANGUAGES = {
    "rust": (
        "Rust",
        [
            ("redis", "rust/Cargo.lock", r'name = "redis"\nversion = "([^"]+)"'),
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

# For example: `test Create/Redis ... bench:  44,055,505 ns/iter (+/- 5,345,991)`
# Accept optional thousands separators.
BENCH_LINE = re.compile(
    r"test\s+(\w+)/(\w+)\s+\.\.\.\s+bench:\s+([\d,]+)\s+ns/iter\s+\(\+/-\s+([\d,]+)\)"
)
METADATA_LINE = re.compile(r"#\s*([\w ]+?)\s*:\s*(.*?)\s*$")
FILE_NAME = re.compile(r"(\w+)-(\d+)-(\w+)")


def parse(text, path="<text>"):
    """Returns ({(operation, implementation): (median ns, std dev ns)}, {key: value})."""
    times, metadata = {}, {}
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
        group, implementation, median, deviation = match.groups()
        operation = group.replace("_", " ")
        if operation not in OPERATIONS or implementation not in IMPLEMENTATION_IDS:
            raise ValueError(f"{path}: unknown benchmark {group}/{implementation}")
        median, deviation = (
            int(median.replace(",", "")),
            int(deviation.replace(",", "")),
        )
        if median <= 0:
            raise ValueError(f"{path}: {group}/{implementation} took no time")
        if (operation, implementation) in times:
            raise ValueError(f"{path}: duplicate benchmark {group}/{implementation}")
        times[(operation, implementation)] = (median, deviation)
        log.info(
            "%s: %s %s: %s ns (+/- %s)",
            path,
            operation,
            implementation,
            median,
            deviation,
        )
    return times, metadata


def load_file(path):
    match = FILE_NAME.fullmatch(path.stem)
    if not match or match.group(1) not in LANGUAGES or match.group(3) not in BACKENDS:
        raise ValueError(
            f"{path}: expected <{'|'.join(LANGUAGES)}>-<links>-<{'|'.join(BACKENDS)}>.txt"
        )
    language, background, backend = match.group(1), int(match.group(2)), match.group(3)
    times, metadata = parse(path.read_text(encoding="utf-8"), path)
    expected = {
        (operation, implementation)
        for operation in OPERATIONS
        for implementation, _, _ in BACKENDS[backend]
    } - NOT_MEASURED
    if set(times) != expected:
        missing = sorted(expected - set(times))
        unexpected = sorted(set(times) - expected)
        raise ValueError(
            f"{path}: missing results {missing}, unexpected results {unexpected}"
        )
    return language, background, backend, times, metadata


def validate_result(language, background, result):
    missing = set(BACKENDS) - set(result["metadata"])
    if missing:
        raise ValueError(
            f"{language}, {background} background links: no {', '.join(sorted(missing))} results"
        )
    links = links_per_iteration(result)
    samples = {metadata.get("samples") for metadata in result["metadata"].values()}
    if (
        not 0 < links <= background
        or len(samples) != 1
        or None in samples
        or int(next(iter(samples))) < 3
    ):
        raise ValueError(
            f"{language}, {background}: invalid or mismatched workload metadata"
        )
    for metadata in result["metadata"].values():
        if metadata.get("background") != str(background):
            raise ValueError("background metadata differs from file name")
        validate_provenance(metadata)
    if not result["metadata"]["redis"].get("redis"):
        raise ValueError("missing Redis server version")


def validate_provenance(metadata):
    for field in ("cpu", "date", "runtime", "commit"):
        if not metadata.get(field):
            raise ValueError(f"missing provenance: {field}")


def validate_languages(results):
    sizes = [
        {background for lang, background in results if lang == language}
        for language in LANGUAGES
    ]
    if sizes[0] != sizes[1]:
        raise ValueError("Rust and C# must have the same background sizes")
    for background in sizes[0]:
        workload = {
            (
                metadata["links"],
                metadata["samples"],
                metadata["commit"],
                metadata.get("run"),
            )
            for (language, size), result in results.items()
            if size == background
            for metadata in result["metadata"].values()
        }
        if len(workload) != 1:
            raise ValueError(
                "languages/backends must use the same workload and source run"
            )


def load(directory):
    """All results in `directory`: {(language, background): {"times", "metadata"}}."""
    results = {}
    for path in sorted(Path(directory).glob("*.txt")):
        language, background, backend, times, metadata = load_file(path)
        result = results.setdefault(
            (language, background), {"times": {}, "metadata": {}}
        )
        result["times"].update(times)
        result["metadata"][backend] = metadata
    if not results:
        raise ValueError(f"no results found in {directory}/")
    for (language, background), result in results.items():
        validate_result(language, background, result)
    validate_languages(results)
    return dict(
        sorted(
            results.items(),
            key=lambda item: (list(LANGUAGES).index(item[0][0]), item[0][1]),
        )
    )


def significant(value):
    """Three significant digits without an exponent: 999.6 is 1000, not 1e+03."""
    return f"{float(f'{value:.3g}'):g}"


def duration(nanoseconds):
    rounded = float(significant(nanoseconds))
    for unit, scale in (("s", 1e9), ("ms", 1e6), ("µs", 1e3)):
        if rounded >= scale:
            return f"{significant(rounded / scale)} {unit}"
    return f"{significant(rounded)} ns"


def ratio(value):
    """`significant`, with thousands separators: 36,600, not 36600."""
    rounded = float(significant(value))
    return f"{rounded:,.0f}" if rounded >= 1000 else significant(rounded)


def comparison(measured, reference):
    """How `measured` compares with `reference`, both (median, std dev)."""
    (median, deviation), (reference_median, reference_deviation) = measured, reference
    overlapping = (
        median - deviation <= reference_median + reference_deviation
        and reference_median - reference_deviation <= median + deviation
    )
    if (
        overlapping
        or max(median, reference_median) / min(median, reference_median) < 1 + NOISE
    ):
        return "≈ same"
    if median < reference_median:
        return f"{ratio(reference_median / median)}× faster"
    return f"{ratio(median / reference_median)}× slower"


def baseline(times, operation):
    """The fastest Redis implementation of `operation`."""
    measured = [
        implementation
        for implementation, _, _ in REDIS
        if (operation, implementation) in times
    ]
    return min(
        measured, key=lambda implementation: times[(operation, implementation)][0]
    )


def table(times):
    header = (
        "| Operation | " + " | ".join(name for _, name, _ in IMPLEMENTATIONS) + " |"
    )
    separator = "| --- |" + " ---: |" * len(IMPLEMENTATIONS)
    rows = [header, separator]
    for operation in OPERATIONS:
        fastest = baseline(times, operation)
        cells = []
        for implementation, _, _ in IMPLEMENTATIONS:
            measured = times.get((operation, implementation))
            if measured is None:
                cells.append("—")
            elif implementation == fastest:
                cells.append(f"**{duration(measured[0])}**")
            elif implementation in DOUBLETS_IDS:
                cells.append(
                    f"{duration(measured[0])} ({comparison(measured, times[(operation, fastest)])})"
                )
            else:
                cells.append(duration(measured[0]))
        rows.append(f"| {operation} | " + " | ".join(cells) + " |")
    return "\n".join(rows)


def versions(language, root=ROOT):
    """`name version` of the Redis driver and the Doublets library of `language`."""
    found = []
    for name, file, pattern in LANGUAGES[language][1]:
        path = root / file
        match = (
            re.search(pattern, path.read_text(encoding="utf-8"))
            if path.exists()
            else None
        )
        found.append(f"{name} {match.group(1) if match else 'unknown version'}")
    return found


def provenance(language, result, links, root=ROOT):
    """Where the numbers come from: versions, machines and the run."""
    redis, doublets = result["metadata"]["redis"], result["metadata"]["doublets"]
    driver, library = versions(language, root)
    cpus = {redis.get("cpu"), doublets.get("cpu")} - {None}
    if not cpus:
        machine = "CPU unknown"
    elif len(cpus) == 1:
        machine = f"CPU {cpus.pop()}"
    else:
        machine = f"CPU {redis.get('cpu', 'unknown')} (Redis) and {doublets.get('cpu', 'unknown')} (Doublets)"
    run, date = (
        redis.get("run") or doublets.get("run"),
        redis.get("date") or doublets.get("date"),
    )
    source = f"[GitHub Actions run]({run})" if run else "a local run"
    if date:
        source += f" on {date}"
    return (
        f"_Median time of one iteration: {links:,} operations (Each All scans once). "
        f"Redis {redis.get('redis', 'unknown version')} through {driver}; {library}. "
        f"{machine}, {source}. "
        f"Runtime {redis.get('runtime')} / {doublets.get('runtime')} (Redis / Doublets); "
        f"{redis.get('samples')} samples, 3 warm-ups. "
        f"Commit `{redis.get('commit')}` / `{doublets.get('commit')}`._"
    )


def links_per_iteration(result):
    counts = {metadata.get("links") for metadata in result["metadata"].values()}
    if len(counts) != 1 or None in counts:
        raise ValueError(
            f"the backends were measured with different or unknown links per iteration: {counts}"
        )
    return int(counts.pop())


def chart_names(language, background):
    return (
        f"bench_{language}_{background}.png",
        f"bench_{language}_log_scale_{background}.png",
    )


def section(results, charts_link):
    """The Markdown between the markers: a heading per language and per number of background links."""
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
            heading = f"{background:,} background links, {links:,} links per iteration"
            lines += [
                f"#### {title}: {heading}",
                "",
                provenance(language, result, links),
                "",
                table(result["times"]),
                "",
            ]
            if charts_link is not None:
                linear, log = chart_names(language, background)
                lines += [
                    f"![{title}, {heading}, linear scale]({charts_link}/{linear})",
                    f"![{title}, {heading}, log scale]({charts_link}/{log})",
                    "",
                ]
    lines += [LINT_ON]
    return "\n".join(lines)


def replace_section(document, generated):
    if document.count(START_MARKER) != 1 or document.count(END_MARKER) != 1:
        raise ValueError("expected exactly one results marker pair")
    pattern = re.compile(
        re.escape(START_MARKER) + ".*?" + re.escape(END_MARKER), re.DOTALL
    )
    if not pattern.search(document):
        raise ValueError(f"{START_MARKER} ... {END_MARKER} markers are missing")
    return pattern.sub(lambda _: f"{START_MARKER}\n{generated}\n{END_MARKER}", document)


def chart(times, title, path, log_scale):
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from matplotlib.ticker import EngFormatter

    positions, width = range(len(OPERATIONS)), 0.8 / len(IMPLEMENTATIONS)
    figure, axis = plt.subplots(figsize=(12, 8))
    # Seconds, so that the axis shows `10 ms` instead of `1e7`.
    values = [
        [
            times.get((operation, implementation), (0, 0))[0] / 1e9
            for operation in OPERATIONS
        ]
        for implementation, _, _ in IMPLEMENTATIONS
    ]
    for index, ((_, name, color), series) in enumerate(
        zip(IMPLEMENTATIONS, values, strict=True)
    ):
        offset = (index - (len(IMPLEMENTATIONS) - 1) / 2) * width
        axis.barh(
            [position + offset for position in positions],
            series,
            width,
            label=name,
            color=color,
        )
    axis.set_xlabel(
        "Median time of one iteration" + (", log scale" if log_scale else "")
    )
    if log_scale:
        axis.set_xscale("log")
    axis.xaxis.set_major_formatter(EngFormatter(unit="s"))
    axis.grid(axis="x", alpha=0.3)
    axis.set_title(title)
    axis.set_yticks(list(positions))
    axis.set_yticklabels(OPERATIONS)
    axis.invert_yaxis()
    axis.legend(loc="upper left", bbox_to_anchor=(1.01, 1))
    figure.tight_layout()
    figure.savefig(path)
    plt.close(figure)
    log.info("%s saved", path)


def charts(results, directory):
    directory.mkdir(parents=True, exist_ok=True)
    written = []
    for (language, background), result in results.items():
        title = (
            f"{LANGUAGES[language][0]}: {background:,} background links, "
            f"{links_per_iteration(result):,} links per iteration"
        )
        linear, log = chart_names(language, background)
        chart(result["times"], title, directory / linear, log_scale=False)
        chart(result["times"], title, directory / log, log_scale=True)
        written += [directory / linear, directory / log]
    return written


def main(arguments=None):
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "results", type=Path, help="directory with the benchmark output"
    )
    parser.add_argument("--readme", type=Path, help="README to update in place")
    parser.add_argument("--charts", type=Path, help="directory for the charts")
    options = parser.parse_args(arguments)
    results = load(options.results)
    if options.charts is not None:
        charts(results, options.charts)
    link = None
    if options.charts is not None:
        base = options.readme.resolve().parent if options.readme else Path.cwd()
        link = Path(os.path.relpath(options.charts.resolve(), base)).as_posix()
    generated = section(results, link)
    print(generated)
    if options.readme is not None:
        document = options.readme.read_text(encoding="utf-8")
        options.readme.write_text(
            replace_section(document, generated), encoding="utf-8"
        )


if __name__ == "__main__":
    main()

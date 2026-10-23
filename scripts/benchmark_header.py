#!/usr/bin/env python3
"""Prints the `# key: value` lines that describe a benchmark run, for
`scripts/benchmark_report.py`.

Usage: benchmark_header.py redis|doublets

The sizes come from `BENCHMARK_LINKS` and `BENCHMARK_BACKGROUND_LINKS` (the
defaults of the benchmarks), the Redis server from `REDIS_URL` (default
`redis://localhost:6379`), whose version is asked with `INFO server`.
"""

from __future__ import annotations

import datetime
import os
import re
import socket
import subprocess  # nosec B404
import sys
from urllib.parse import urlsplit


def redis_version(url: str, timeout: float = 5.0) -> str:
    """The `redis_version` of `INFO server`, or `unknown version` if the server does not answer."""
    address = urlsplit(url)
    try:
        with socket.create_connection((address.hostname or "localhost", address.port or 6379), timeout) as server:
            if address.password:
                user = address.username or "default"
                server.sendall(command("AUTH", user, address.password))
                read_reply(server)
            server.sendall(command("INFO", "server"))
            info = read_reply(server)
    except (OSError, ValueError) as error:
        print(f"Cannot ask Redis at {address.hostname}:{address.port} for its version: {error}", file=sys.stderr)
        return "unknown version"
    match = re.search(r"^redis_version:(\S+)", info, re.MULTILINE)
    return match.group(1) if match else "unknown version"


def command(*arguments: str) -> bytes:
    """A command in the Redis protocol (RESP)."""
    parts = [f"*{len(arguments)}\r\n".encode()]
    for argument in arguments:
        data = argument.encode()
        parts.append(b"$%d\r\n%s\r\n" % (len(data), data))
    return b"".join(parts)


def read_reply(server: socket.socket) -> str:
    """Reads one simple or bulk string reply; an error reply raises `ValueError`."""
    data = b""
    while b"\r\n" not in data:
        data += receive(server)
    line, rest = data.split(b"\r\n", 1)
    if line.startswith(b"-"):
        raise ValueError(line[1:].decode(errors="replace"))
    if line.startswith(b"+"):
        return line[1:].decode()
    if not line.startswith(b"$"):
        raise ValueError(f"unexpected reply {line[:20]!r}")
    length = int(line[1:])
    while len(rest) < length + 2:
        rest += receive(server)
    return rest[:length].decode(errors="replace")


def receive(server: socket.socket) -> bytes:
    chunk = server.recv(65536)
    if not chunk:
        raise ValueError("the connection was closed")
    return chunk


def cpu() -> str:
    try:
        output = subprocess.run(["lscpu"], capture_output=True, text=True, check=False).stdout  # nosec B603 B607
    except OSError:
        return "unknown"
    match = re.search(r"^Model name:\s*(.+)$", output, re.MULTILINE)
    return match.group(1).strip() if match else "unknown"


def header(backend: str, environment: os._Environ[str] | dict[str, str] = os.environ) -> list[str]:
    lines = [
        f"# links: {environment.get('BENCHMARK_LINKS', '100')}",
        f"# background links: {environment.get('BENCHMARK_BACKGROUND_LINKS', '1000')}",
        f"# cpu: {cpu()}",
    ]
    if backend == "redis":
        lines.append(f"# redis: {redis_version(environment.get('REDIS_URL', 'redis://localhost:6379'))}")
    lines.append(f"# date: {datetime.datetime.now(datetime.UTC):%Y-%m-%d}")
    if environment.get("GITHUB_RUN_ID"):
        server = environment.get("GITHUB_SERVER_URL", "https://github.com")
        repository = environment.get("GITHUB_REPOSITORY", "")
        lines.append(f"# run: {server}/{repository}/actions/runs/{environment['GITHUB_RUN_ID']}")
    return lines


def main(arguments: list[str]) -> None:
    if len(arguments) != 1 or arguments[0] not in ("redis", "doublets"):
        sys.exit("usage: benchmark_header.py redis|doublets")
    print("\n".join(header(arguments[0])), flush=True)


if __name__ == "__main__":
    main(sys.argv[1:])

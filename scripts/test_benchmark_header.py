"""Tests of benchmark_header.py: python3 -m unittest discover -s scripts"""

import socket
import threading
import unittest

import benchmark_header as header

INFO = b"# Server\r\nredis_version:8.10.2\r\nredis_mode:standalone\r\n"


def serve(replies):
    """A server on a free port that answers every command with the next reply; returns its URL and the
    commands it received."""
    listener = socket.create_server(("127.0.0.1", 0))
    received = []

    def answer():
        connection, _ = listener.accept()
        with connection, listener:
            for reply in replies:
                received.append(connection.recv(65536))
                # Sent in two parts, as a reply may arrive.
                connection.sendall(reply[:3])
                connection.sendall(reply[3:])

    threading.Thread(target=answer, daemon=True).start()
    return f"redis://127.0.0.1:{listener.getsockname()[1]}", received


class Protocol(unittest.TestCase):
    def test_commands_are_encoded_in_resp(self):
        self.assertEqual(header.command("INFO", "server"), b"*2\r\n$4\r\nINFO\r\n$6\r\nserver\r\n")

    def test_the_version_comes_from_info(self):
        url, received = serve([b"$%d\r\n%s\r\n" % (len(INFO), INFO)])
        self.assertEqual(header.redis_version(url), "8.10.2")
        self.assertEqual(received, [header.command("INFO", "server")])

    def test_a_password_is_sent_first(self):
        url, received = serve([b"+OK\r\n", b"$%d\r\n%s\r\n" % (len(INFO), INFO)])
        url = url.replace("redis://", "redis://:secret@")
        self.assertEqual(header.redis_version(url), "8.10.2")
        self.assertEqual(received[0], header.command("AUTH", "default", "secret"))

    def test_an_error_reply_gives_an_unknown_version(self):
        url, _ = serve([b"-NOAUTH Authentication required.\r\n"])
        self.assertEqual(header.redis_version(url), "unknown version")

    def test_no_server_gives_an_unknown_version(self):
        with socket.create_server(("127.0.0.1", 0)) as unused:
            port = unused.getsockname()[1]
        self.assertEqual(header.redis_version(f"redis://127.0.0.1:{port}", timeout=1), "unknown version")


class Header(unittest.TestCase):
    def test_defaults(self):
        lines = header.header("doublets", {})
        self.assertEqual(lines[:2], ["# links: 100", "# background links: 1000"])
        self.assertTrue(lines[2].startswith("# cpu: "))
        self.assertRegex(lines[3], r"^# date: \d{4}-\d{2}-\d{2}$")
        self.assertEqual(len(lines), 4)

    def test_a_ci_run_is_linked(self):
        url, _ = serve([b"$%d\r\n%s\r\n" % (len(INFO), INFO)])
        environment = {
            "BENCHMARK_LINKS": "1000",
            "BENCHMARK_BACKGROUND_LINKS": "100000",
            "REDIS_URL": url,
            "GITHUB_RUN_ID": "7",
            "GITHUB_REPOSITORY": "owner/repo",
        }
        lines = header.header("redis", environment)
        self.assertEqual(lines[:2], ["# links: 1000", "# background links: 100000"])
        self.assertIn("# redis: 8.10.2", lines)
        self.assertEqual(lines[-1], "# run: https://github.com/owner/repo/actions/runs/7")


if __name__ == "__main__":
    unittest.main()

"""Tests of audit_dotnet.py: python3 -m unittest discover -s scripts"""

import unittest

import audit_dotnet as audit


def report(*packages, group="transitivePackages"):
    return {"projects": [{"path": "a.csproj", "frameworks": [{"framework": "net10.0", group: list(packages)}]}]}


class Vulnerabilities(unittest.TestCase):
    def test_a_clean_report_has_none(self):
        self.assertEqual(audit.vulnerabilities({"projects": [{"path": "a.csproj"}]}), [])

    def test_direct_and_transitive_packages_are_reported(self):
        advisory = {"severity": "High", "advisoryurl": "https://example.com/1"}
        for group in ("topLevelPackages", "transitivePackages"):
            found = audit.vulnerabilities(report({"id": "Bad", "vulnerabilities": [advisory]}, group=group))
            self.assertEqual(found, [("a.csproj", "Bad", "High", "https://example.com/1")])

    def test_an_incomplete_report_fails(self):
        with self.assertRaises(ValueError):
            audit.vulnerabilities({"projects": []})
        with self.assertRaises(ValueError):
            audit.vulnerabilities({"problems": [{"text": "no assets file"}], "projects": [{"path": "a.csproj"}]})

    def test_every_project_is_found(self):
        names = {path.name for path in audit.projects()}
        self.assertIn("RedisVSDoublets.csproj", names)
        self.assertIn("RedisVSDoublets.Tests.csproj", names)


if __name__ == "__main__":
    unittest.main()

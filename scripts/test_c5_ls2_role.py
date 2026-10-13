#!/usr/bin/env python3
"""Offline regression checks for the narrowly scoped C5 LS2 registration role."""
import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASE = ROOT / "security/c5-registration-only"


class C5Ls2RoleTests(unittest.TestCase):
    def test_registration_role_is_exact_executable_and_minimal(self):
        role = json.loads((BASE / "role.json").read_text())
        self.assertEqual(role["type"], "regular")
        self.assertEqual(role["exeName"], "/var/home/root/c5-histogram-probe")
        self.assertEqual(role["allowedNames"], ["com.webos.rm.client.*"])
        self.assertEqual(
            role["permissions"],
            [
                {
                    "service": "com.webos.rm.client.*",
                    "outbound": ["com.webos.media"],
                }
            ],
        )
        self.assertNotIn("inbound", role["permissions"][0])
        serialized = json.dumps(role)
        for forbidden in (
            '"privileged"',
            '"all"',
            '"com.webos.service.capture"',
            '"com.webos.service.capturepermission"',
            '"*"',
        ):
            self.assertNotIn(forbidden, serialized)

    def test_stage2_does_not_expand_outbound_permissions(self):
        baseline = json.loads((BASE / "role.json").read_text())
        role = json.loads((BASE / "stage2-role.json").read_text())
        self.assertEqual(role["type"], "regular")
        self.assertEqual(role["exeName"], baseline["exeName"])
        self.assertEqual(
            role["allowedNames"],
            ["com.webos.rm.client.*", "com.webos.service.capture.client*"],
        )
        self.assertEqual(
            role["permissions"],
            [
                baseline["permissions"][0],
                {
                    "service": "com.webos.service.capture.client*",
                    "outbound": [],
                },
            ],
        )
        for item in role["permissions"]:
            self.assertNotIn("inbound", item)
        serialized = json.dumps(role)
        for forbidden in (
            '"privileged"',
            '"all"',
            '"capturepermission.security"',
            '"com.webos.service.capturepermission"',
            '"outbound": ["*"]',
        ):
            self.assertNotIn(forbidden, serialized)

    def test_manifest_installs_only_one_dev_role(self):
        manifest = json.loads((BASE / "manifest.json").read_text())
        self.assertEqual(
            manifest["roleFiles"],
            [
                "/var/luna-service2-dev/roles.d/"
                "org.webosbrew.lg-hue-sync.c5-rm-probe.role.json"
            ],
        )
        for field in (
            "serviceFiles",
            "clientPermissionFiles",
            "apiPermissionFiles",
            "groupsFiles",
        ):
            self.assertEqual(manifest[field], [])


if __name__ == "__main__":
    unittest.main()

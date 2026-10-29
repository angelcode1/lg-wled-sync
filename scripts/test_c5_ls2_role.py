#!/usr/bin/env python3
"""Offline regression checks for the narrowly scoped C5 LS2 registration role."""
import json
import re
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


class OwnedNameRoleTests(unittest.TestCase):
    BASE = ROOT / "security/c5-own-name-registration"
    NAME = "org.webosbrew.lg-hue-sync.c5-probe"

    def test_owned_role_matches_runtime_identity_without_vendor_names(self):
        role = json.loads((self.BASE / "role.json").read_text())
        source = (ROOT / "src/ls2_probe.rs").read_text()
        name = re.search(r'const OWN_PROBE_NAME: &str = "([^"]+)";', source)
        self.assertIsNotNone(name)
        self.assertEqual(name[1], self.NAME)
        self.assertEqual(role["exeName"], "/var/home/root/c5-ls2-probe")
        self.assertEqual(role["type"], "regular")
        self.assertEqual(role["trustLevel"], "dev")
        self.assertEqual(role["allowedNames"], [self.NAME])
        self.assertEqual(role["permissions"], [
            {"service": self.NAME, "inbound": [], "outbound": []}
        ])
        self.assertNotIn("*", json.dumps(role))
        self.assertNotIn("com.webos.", json.dumps(role))

    def test_owned_client_has_no_access_control_groups(self):
        permission = json.loads((self.BASE / "client-permissions.json").read_text())
        self.assertEqual(permission, {self.NAME: []})

    def test_owned_manifest_references_only_its_role_and_empty_group_client(self):
        manifest = json.loads((self.BASE / "manifest.json").read_text())
        self.assertEqual(manifest["id"], self.NAME)
        self.assertEqual(manifest["roleFiles"], [
            f"/var/luna-service2/roles.d/{self.NAME}.role.json"
        ])
        self.assertEqual(manifest["clientPermissionFiles"], [
            f"/var/luna-service2/client-permissions.d/{self.NAME}.perm.json"
        ])
        for field in ("serviceFiles", "apiPermissionFiles", "groupsFiles"):
            self.assertEqual(manifest[field], [])


if __name__ == "__main__":
    unittest.main()

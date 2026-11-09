import copy
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPT = HERE.parent / "scripts" / "compose-scope.py"
FIXTURE = json.loads((HERE / "fixtures" / "compose" / "devnet.json").read_text())
PROJECT = "forest-review-k-devnet"
NAMES = ["--container-name", "forest", "--container-name", "lotus"]


def run(config, *extra):
    text = config if isinstance(config, str) else json.dumps(config)
    return subprocess.run(
        [sys.executable, str(SCRIPT), "--project", PROJECT, "--worktree", "/w", *extra],
        input=text, capture_output=True, text=True, check=False,
    )


def mutated(fn):
    config = copy.deepcopy(FIXTURE)
    fn(config)
    return config


class ComposeScopeTest(unittest.TestCase):
    def assert_refused(self, config, needle, *extra):
        res = run(config, *(extra or NAMES))
        self.assertEqual(res.returncode, 1, res.stdout + res.stderr)
        self.assertIn(needle, res.stdout)

    def test_clean_config_passes(self):
        res = run(FIXTURE, *NAMES)
        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
        self.assertEqual(res.stdout.strip(), "ok")

    def test_unlisted_container_names_are_refused(self):
        res = run(FIXTURE)
        self.assertEqual(res.returncode, 1)
        self.assertIn("service forest: container_name forest", res.stdout)
        self.assertIn("service lotus_node: container_name lotus", res.stdout)

    def test_explicit_volume_name_outside_project_is_refused(self):
        def f(c):
            c["volumes"]["lotus-data"]["name"] = "api_compare_node-data"
        self.assert_refused(mutated(f), "volume lotus-data: name api_compare_node-data")

    def test_external_volume_is_refused(self):
        def f(c):
            c["volumes"]["lotus-data"]["external"] = True
        self.assert_refused(mutated(f), "volume lotus-data: external")

    def test_volume_with_driver_opts_is_refused(self):
        def f(c):
            c["volumes"]["lotus-data"]["driver_opts"] = {"type": "none", "o": "bind", "device": "/elsewhere/data"}
        self.assert_refused(mutated(f), "volume lotus-data: driver_opts")

    def test_network_name_outside_project_is_refused(self):
        def f(c):
            c["networks"]["devnet"]["name"] = "devnet"
        self.assert_refused(mutated(f), "network devnet: name devnet")

    def test_external_network_is_refused(self):
        def f(c):
            c["networks"]["devnet"]["external"] = True
        self.assert_refused(mutated(f), "network devnet: external")

    def test_bind_sources_outside_worktree_are_refused(self):
        for source in ["/elsewhere/x", "/w/../etc", "/w2/x"]:
            with self.subTest(source=source):
                def f(c, source=source):
                    c["services"]["lotus_node"]["volumes"].append({"type": "bind", "source": source, "target": "/x"})
                self.assert_refused(mutated(f), f"service lotus_node: bind source {source}")

    def test_docker_socket_is_refused(self):
        def f(c):
            c["services"]["forest"]["volumes"].append({"type": "bind", "source": "/var/run/docker.sock", "target": "/var/run/docker.sock"})
        self.assert_refused(mutated(f), "service forest: mounts the docker socket")

    def test_host_level_settings_are_refused(self):
        cases = [
            ("privileged", True, "privileged"),
            ("network_mode", "host", "network_mode host"),
            ("network_mode", "container:other", "network_mode container:other"),
            ("pid", "host", "pid host"),
            ("ipc", "host", "ipc host"),
            ("devices", [{"source": "/dev/fuse", "target": "/dev/fuse"}], "devices"),
            ("volumes_from", ["container:other"], "volumes_from container:other"),
        ]
        for key, value, needle in cases:
            with self.subTest(key=key, value=value):
                def f(c, key=key, value=value):
                    c["services"]["lotus_init"][key] = value
                self.assert_refused(mutated(f), f"service lotus_init: {needle}")

    def test_joining_another_container_namespace_is_refused(self):
        for key in ["pid", "ipc", "uts", "cgroup", "userns_mode", "network_mode"]:
            with self.subTest(key=key):
                def f(c, key=key):
                    c["services"]["lotus_init"][key] = "container:user-db"
                self.assert_refused(mutated(f), f"service lotus_init: {key} container:user-db")

    def test_service_network_mode_inside_project_passes(self):
        def f(c):
            c["services"]["lotus_init"]["network_mode"] = "service:lotus_node"
        self.assertEqual(run(mutated(f), *NAMES).returncode, 0)

    def test_built_image_tag_outside_project_is_refused(self):
        def f(c):
            c["services"]["forest"]["image"] = "ghcr.io/chainsafe/forest:edge-fat"
        self.assert_refused(mutated(f), "service forest: built image ghcr.io/chainsafe/forest:edge-fat")

    def test_built_image_tag_inside_project_passes(self):
        def f(c):
            c["services"]["forest"]["image"] = f"{PROJECT}-forest"
        self.assertEqual(run(mutated(f), *NAMES).returncode, 0)

    def test_pull_policy_that_replaces_local_tags_is_refused(self):
        def f(c):
            c["services"]["lotus_node"]["pull_policy"] = "always"
        self.assert_refused(mutated(f), "service lotus_node: pull_policy always")

        def g(c):
            c["services"]["lotus_node"]["pull_policy"] = "missing"
        self.assertEqual(run(mutated(g), *NAMES).returncode, 0)

    def test_capabilities_and_security_options_are_refused(self):
        for key, value in [("cap_add", ["SYS_ADMIN"]), ("security_opt", ["seccomp:unconfined"])]:
            with self.subTest(key=key):
                def f(c, key=key, value=value):
                    c["services"]["lotus_init"][key] = value
                self.assert_refused(mutated(f), f"service lotus_init: {key}")

    def test_build_reaching_outside_worktree_is_refused(self):
        cases = [
            ("context", "/elsewhere", "build context /elsewhere"),
            ("ssh", ["default"], "build ssh"),
            ("additional_contexts", {"extra": "/elsewhere"}, "build additional context extra /elsewhere"),
            ("additional_contexts", {"img": "docker-image://alpine"}, "build additional context img docker-image://alpine"),
        ]
        for key, value, needle in cases:
            with self.subTest(key=key, value=value):
                def f(c, key=key, value=value):
                    c["services"]["forest"]["build"][key] = value
                self.assert_refused(mutated(f), f"service forest: {needle}")

    def test_writable_directory_bind_is_refused(self):
        with tempfile.TemporaryDirectory() as wt:
            (Path(wt) / "sub").mkdir()
            config = copy.deepcopy(FIXTURE)
            config["services"]["forest"]["build"]["context"] = wt
            config["services"]["forest"]["volumes"] = [{"type": "bind", "source": f"{wt}/sub", "target": "/sub"}]
            res = subprocess.run(
                [sys.executable, str(SCRIPT), "--project", PROJECT, "--worktree", wt, *NAMES],
                input=json.dumps(config), capture_output=True, text=True, check=False,
            )
            self.assertEqual(res.returncode, 1)
            self.assertIn(f"service forest: bind source {wt}/sub is a writable directory", res.stdout)
            config["services"]["forest"]["volumes"][0]["read_only"] = True
            res = subprocess.run(
                [sys.executable, str(SCRIPT), "--project", PROJECT, "--worktree", wt, *NAMES],
                input=json.dumps(config), capture_output=True, text=True, check=False,
            )
            self.assertEqual(res.returncode, 0, res.stdout)

    def test_secret_file_outside_worktree_is_refused(self):
        def f(c):
            c["secrets"] = {"key": {"file": "/elsewhere/.ssh/id_ed25519"}}
        self.assert_refused(mutated(f), "secrets key: file /elsewhere/.ssh/id_ed25519")

    def test_invalid_json_exits_2(self):
        self.assertEqual(run("not json", *NAMES).returncode, 2)
        self.assertEqual(run("", *NAMES).returncode, 2)


if __name__ == "__main__":
    unittest.main()

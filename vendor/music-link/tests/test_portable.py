"""Portable setup and POSIX process ownership regressions; no audio/build tools."""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "music-link"))


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


setup = module("portable_setup", ROOT / "vendor/setup.py")
start = module("portable_start", ROOT / "music-link/start_music.py")
build_inputs = module("build_inputs_check", ROOT / "music-link/build_inputs.py")


class SetupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="music workspace with spaces ")
        self.root = Path(self.temporary.name)
        self.manifest = {"version": 1, "projects": []}
        for name in ("one", "two"):
            repo = self.root / name
            repo.mkdir()
            def git(*args):
                return subprocess.check_output(["git", "-C", str(repo), *args], stderr=subprocess.DEVNULL)
            git("init", "-q")
            git("config", "core.autocrlf", "false")
            git("config", "user.name", "Portable test")
            git("config", "user.email", "test@example.invalid")
            (repo / "code.txt").write_bytes(b"original\n")
            git("add", "code.txt")
            git("commit", "-qm", "base")
            base = git("rev-parse", "HEAD").decode().strip()
            (repo / "code.txt").write_bytes(b"integrated\n")
            patch = git("diff", "--binary", "--full-index")
            folder = self.root / "vendor" / name
            (folder / "files").mkdir(parents=True)
            (folder / "changes.patch").write_bytes(patch)
            (folder / "files/code.txt").write_bytes(b"integrated\n")
            (repo / "code.txt").write_bytes(b"original\n")
            self.manifest["projects"].append({"name": name, "base": base,
                "patch_sha256": hashlib.sha256(patch).hexdigest(),
                "files": [{"path": "code.txt", "sha256": hashlib.sha256(b"integrated\n").hexdigest()}]})
        (self.root / "vendor/manifest.json").write_text(json.dumps(self.manifest))

    def tearDown(self):
        self.temporary.cleanup()

    def apply(self, **kwargs):
        with contextlib.redirect_stdout(io.StringIO()):
            setup.apply_workspace(root=self.root, **kwargs)

    def test_clean_checkout_applies_and_repeat_is_idempotent(self):
        self.apply()
        self.apply()
        self.assertEqual((self.root / "one/code.txt").read_bytes(), b"integrated\n")

    def test_check_does_not_change_source(self):
        self.apply(check_only=True)
        self.assertEqual((self.root / "one/code.txt").read_bytes(), b"original\n")

    def test_conflict_in_second_project_prevents_first_project_changes(self):
        (self.root / "two/code.txt").write_bytes(b"my local changes\n")
        with self.assertRaisesRegex(RuntimeError, "conflicting local edits"):
            self.apply()
        self.assertEqual((self.root / "one/code.txt").read_bytes(), b"original\n")
        self.assertEqual((self.root / "two/code.txt").read_bytes(), b"my local changes\n")

    def test_corrupt_vendor_copy_is_rejected_before_any_apply(self):
        (self.root / "vendor/two/files/code.txt").write_bytes(b"corrupted\n")
        with self.assertRaisesRegex(RuntimeError, "checksum mismatch"):
            self.apply()
        self.assertEqual((self.root / "one/code.txt").read_bytes(), b"original\n")

    def test_manifest_path_cannot_escape_source_copy_directory(self):
        with self.assertRaisesRegex(RuntimeError, "escapes workspace"):
            setup.checked_path(self.root, "../outside")

    def prepare_old_snapshot_upgrade(self):
        def git(*args):
            return subprocess.check_output(["git", "-C", str(self.root), *args], stderr=subprocess.DEVNULL)
        git("init", "-q")
        git("config", "user.name", "Portable test")
        git("config", "user.email", "test@example.invalid")
        git("add", "vendor")
        git("commit", "-qm", "previous vendored snapshot")
        for project in self.manifest["projects"]:
            repo = self.root / project["name"]
            (repo / "code.txt").write_bytes(b"updated integration\n")
            patch = subprocess.check_output(["git", "-C", str(repo), "diff", "--binary", "--full-index"])
            folder = self.root / "vendor" / project["name"]
            (folder / "changes.patch").write_bytes(patch)
            (folder / "files/code.txt").write_bytes(b"updated integration\n")
            project["patch_sha256"] = hashlib.sha256(patch).hexdigest()
            project["files"][0]["sha256"] = hashlib.sha256(b"updated integration\n").hexdigest()
            (repo / "code.txt").write_bytes(b"integrated\n")
        (self.root / "vendor/manifest.json").write_text(json.dumps(self.manifest))

    def test_upgrade_applies_new_snapshot_to_previous_integration(self):
        self.prepare_old_snapshot_upgrade()
        self.apply(upgrade=True)
        self.assertEqual((self.root / "one/code.txt").read_bytes(), b"updated integration\n")

    def test_upgrade_accepts_a_relative_workspace_root(self):
        self.prepare_old_snapshot_upgrade()
        previous = Path.cwd()
        output = io.StringIO()
        try:
            os.chdir(self.root.parent)
            with contextlib.redirect_stdout(output):
                setup.apply_workspace(root=Path(self.root.name), upgrade=True)
        finally:
            os.chdir(previous)
        for name in ("one", "two"):
            self.assertEqual((self.root / name / "code.txt").read_bytes(), b"updated integration\n")
        self.assertIn("Updated vendored source:", output.getvalue())

    @unittest.skipIf(os.name == "nt", "POSIX workspace symlink; relative-root regression also runs on Windows")
    def test_upgrade_accepts_a_symlinked_workspace_root(self):
        self.prepare_old_snapshot_upgrade()
        alias = self.root / "workspace alias"
        alias.symlink_to(self.root, target_is_directory=True)
        with contextlib.redirect_stdout(io.StringIO()):
            setup.apply_workspace(root=alias, upgrade=True)
        for name in ("one", "two"):
            self.assertEqual((self.root / name / "code.txt").read_bytes(), b"updated integration\n")

    def test_upgrade_preserves_unknown_edits_before_any_write(self):
        self.prepare_old_snapshot_upgrade()
        (self.root / "two/code.txt").write_bytes(b"my custom source\n")
        with self.assertRaisesRegex(RuntimeError, "conflicting local edits"):
            self.apply(upgrade=True)
        self.assertEqual((self.root / "one/code.txt").read_bytes(), b"integrated\n")
        self.assertEqual((self.root / "two/code.txt").read_bytes(), b"my custom source\n")

    def test_binary_is_stale_after_source_update(self):
        artifact = self.root / "app"
        artifact.write_bytes(b"old build")
        os.utime(artifact, ns=(1, 1))
        self.assertTrue(build_inputs.needs_build("one", artifact, self.root))
        os.utime(artifact, ns=(time.time_ns()+1_000_000_000, time.time_ns()+1_000_000_000))
        self.assertFalse(build_inputs.needs_build("one", artifact, self.root))


class RustelStartupTests(unittest.TestCase):
    def test_waits_past_the_old_deadline_for_the_selected_players_lanes(self):
        with tempfile.TemporaryDirectory() as directory:
            launcher = start.Launcher(Path(directory))
            clock = 0.0

            def sleep(seconds):
                nonlocal clock
                clock += seconds

            def sessions():
                return [{"session": "rustel-123-other", "lanes": [{}]},
                        {"session": "rustel-12-selected", "lanes": [{}] if clock >= 60 else []}]

            with mock.patch.object(start, "live_record", return_value=True), \
                 mock.patch.object(start, "hub_state", side_effect=sessions), \
                 mock.patch.object(start.time, "monotonic", side_effect=lambda: clock), \
                 mock.patch.object(start.time, "sleep", side_effect=sleep), \
                 contextlib.redirect_stdout(io.StringIO()):
                launcher.wait_for_rustel_lanes({"pid": 12})
            self.assertGreaterEqual(clock, 60)

    def test_reports_when_player_exits_while_waiting(self):
        with tempfile.TemporaryDirectory() as directory:
            launcher = start.Launcher(Path(directory))
            with mock.patch.object(start, "live_record", side_effect=[True, False]), \
                 mock.patch.object(start, "hub_state", return_value=[]), \
                 contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaisesRegex(RuntimeError, "exited before publishing lanes.*rustel-error.log"):
                    launcher.wait_for_rustel_lanes({"pid": 12})


class ViewerStartupTests(unittest.TestCase):
    def test_relaunch_reopens_browser_and_prints_url_despite_old_saved_flag(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "music-link").mkdir()
            launcher = start.Launcher(root)
            launcher.state["web_opened"] = True  # Old launcher state must not suppress opening.
            args = SimpleNamespace(build=False, web_port=18765, no_open=False)
            output = io.StringIO()
            with mock.patch.object(start, "build_viewer"), \
                 mock.patch.object(start.HTTP, "open", side_effect=lambda *a, **k: io.BytesIO(b"<title>Nightshade Viewer</title>")), \
                 mock.patch.object(start.webbrowser, "open", return_value=True) as browser, \
                 contextlib.redirect_stdout(output):
                launcher.start_web(args)
                launcher.start_web(args)
            self.assertEqual(browser.call_count, 2)
            browser.assert_called_with("http://127.0.0.1:18765/?music=1")
            self.assertEqual(output.getvalue().count("Nightshade viewer: http://127.0.0.1:18765/?music=1"), 2)

    def test_missing_server_is_started_and_no_open_still_prints_url(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "music-link").mkdir()
            launcher = start.Launcher(root)
            args = SimpleNamespace(build=False, web_port=18766, no_open=True)
            output = io.StringIO()
            with mock.patch.object(start, "build_viewer"), \
                 mock.patch.object(start.HTTP, "open", side_effect=start.urllib.error.URLError("refused")), \
                 mock.patch.object(launcher, "ensure") as server, \
                 mock.patch.object(start, "page_ready", return_value=True), \
                 mock.patch.object(start.webbrowser, "open") as browser, \
                 contextlib.redirect_stdout(output):
                launcher.start_web(args)
            server.assert_called_once()
            browser.assert_not_called()
            self.assertIn("http://127.0.0.1:18766/?music=1", output.getvalue())


@unittest.skipUnless(sys.platform in ("linux", "darwin"), "POSIX process identity")
class ProcessTests(unittest.TestCase):
    def test_darwin_zombie_is_not_a_live_process(self):
        with mock.patch.object(start.sys, "platform", "darwin"), \
             mock.patch.object(start.subprocess, "run", return_value=SimpleNamespace(
                 returncode=0, stdout="Z Sun Nov 22 22:47:00 2026 /path with spaces/rustel\n")):
            self.assertIsNone(start.process_identity(42))

    def test_stop_attempts_bespoke_and_hub_even_if_rustel_times_out(self):
        launcher = start.Launcher()
        bespoke = {"role": "bespoke", "pid": 1, "owned": True}
        rustel = {"role": "rustel", "pid": 2, "owned": True}
        hub = {"role": "hub", "pid": 3, "owned": True}
        launcher.state["processes"] = [hub, rustel, bespoke]

        def wait_for_exit(check, timeout, message):
            if timeout == 15:
                raise RuntimeError(message)

        with mock.patch.object(start.sys, "platform", "darwin"), \
             mock.patch.object(start, "hub_state", return_value=[
                 {"session": "rustel-2-test", "lane_audio": {"mapping": "/rla-2-1"}}]), \
             mock.patch.object(start, "live_record", return_value=True), \
             mock.patch.object(start, "quit_bespoke") as quit_bespoke, \
             mock.patch.object(start.os, "kill") as kill, \
             mock.patch.object(start, "wait_for", side_effect=wait_for_exit), \
             mock.patch.object(start, "unlink_stopped_mapping") as unlink, \
             contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaisesRegex(RuntimeError, "Rustel .* is still running"):
                launcher.stop()
        quit_bespoke.assert_called_once_with(bespoke)
        self.assertEqual(kill.call_args_list, [mock.call(2, signal.SIGTERM), mock.call(3, signal.SIGTERM)])
        unlink.assert_not_called()

    def test_reused_pid_identity_does_not_authorize_stop(self):
        with subprocess.Popen([sys.executable, "-c", "import time;print('ready',flush=True);time.sleep(10)"],
                              stdout=subprocess.PIPE, text=True) as process:
            try:
                self.assertEqual(process.stdout.readline().strip(), "ready")
                identity = start.wait_for(lambda: start.process_identity(process.pid), 2, "process not ready")
                record = {"pid": process.pid, "identity": dict(identity, started="different start")}
                self.assertFalse(start.live_record(record))
                record["identity"] = identity
                self.assertEqual(start.process_identity(process.pid), identity)
                self.assertTrue(start.live_record(record))
            finally:
                process.terminate()
                process.wait()

    def test_launcher_reuses_owned_helper_and_stop_leaves_unowned_helper(self):
        with tempfile.TemporaryDirectory(prefix="music process test ") as directory:
            root = Path(directory)
            (root / "music-link").mkdir()
            launcher = start.Launcher(root)
            with contextlib.redirect_stdout(io.StringIO()):
                owned = launcher.ensure("web", sys.executable, ["-c", "import time;time.sleep(20)"], discover=False)
                try:
                    again = launcher.ensure("web", sys.executable, ["-c", "raise Exception('duplicate')"], discover=False)
                    self.assertEqual(again["pid"], owned["pid"])
                    with subprocess.Popen([sys.executable, "-c", "import time;time.sleep(20)"]) as other:
                        try:
                            identity = start.wait_for(lambda: start.process_identity(other.pid), 2, "process not ready")
                            launcher.state["processes"].append({"role": "external", "pid": other.pid,
                                                               "identity": identity, "owned": False})
                            launcher.stop()
                            start.wait_for(lambda: not start.live_record(owned), 3, "owned helper did not stop")
                            self.assertIsNone(other.poll())
                        finally:
                            other.terminate()
                            other.wait()
                finally:
                    if start.live_record(owned):
                        os.kill(owned["pid"], signal.SIGTERM)
                    try:
                        os.waitpid(owned["pid"], 0)
                    except ChildProcessError:
                        pass


if __name__ == "__main__":
    unittest.main()

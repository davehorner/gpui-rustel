import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("session_score", ROOT / "music-link/session_score.py")
session_score = importlib.util.module_from_spec(spec)
spec.loader.exec_module(session_score)


class SessionScoreTests(unittest.TestCase):
    def test_each_launch_gets_an_independent_named_copy_and_preserves_the_template(self):
        with tempfile.TemporaryDirectory(prefix="music session with spaces ") as directory:
            root = Path(directory)
            demos = root / "strudel"
            demos.mkdir()
            template = demos / "my demo.strudel"
            original = b'const bpm = 124;\r\n$: s("bd*4")\r\n'
            template.write_bytes(original)
            first = session_score.create_session_score(template, root)
            second = session_score.create_session_score(template, root)
            self.assertNotEqual(first, second)
            self.assertEqual(first.parent, root.resolve() / "sessions")
            self.assertTrue(first.name.startswith("my_demo-"))
            self.assertEqual(first.read_bytes(), original)
            first.write_text('setcpm(45);\n$: s("hh*8")')
            self.assertEqual(second.read_bytes(), original)
            self.assertEqual(template.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()

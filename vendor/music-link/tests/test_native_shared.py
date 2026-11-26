"""Exercise the shipping POSIX C++ mapping reader without an audio device."""
import ctypes
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "music-link"))
from native_ipc import SIZE, open_mapping, posix_lib


@unittest.skipUnless(sys.platform in ("linux", "darwin") and shutil.which("c++"), "POSIX C++ compiler required")
class NativeSharedTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix="native shared check ")
        cls.reader = Path(cls.temporary.name) / "reader"
        args = ["c++", "-std=c++17", "-pthread", "-I", str(ROOT / "vendor/BespokeSynth/files/Source"),
                str(ROOT / "music-link/tests/ipc_reader.cpp"), "-o", str(cls.reader)]
        if sys.platform == "linux":
            args += ["-lrt"]
        subprocess.run(args, check=True)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def mapping(self, pid, size=SIZE, initialize=True):
        library = posix_lib()
        name = f"/rla-{pid:x}-{time.time_ns() & ((1<<64)-1):x}"
        fd = library.shm_open(name.encode(), os.O_RDWR | os.O_CREAT | os.O_EXCL, 0o600)
        self.assertGreaterEqual(fd, 0)
        try:
            os.ftruncate(fd, size)
        finally:
            os.close(fd)
        self.addCleanup(lambda: library.shm_unlink(name.encode()))
        if not initialize:
            return name
        with open_mapping(name) as memory:
            struct.pack_into("<IIIII", memory, 0, 0x524C4131, 1, 48000, 15, 16384)
            struct.pack_into("<QQ", memory, 24, 4096, 1)
            for frame in range(4096-32, 4096):
                for slot, samples in enumerate(((0.125,-0.25),(0.25,0.5),(0.0,0.0))):
                    for ch, sample in enumerate(samples):
                        bits = struct.unpack("<I", struct.pack("<f", sample))[0]
                        offset = 4096 + ((slot*16384 + frame)*2+ch)*8
                        struct.pack_into("<Q", memory, offset, (frame<<32)|bits)
        return name

    def test_readers_accept_page_padding(self):
        # Reproduce Darwin's larger reported allocation on Linux as well.
        name = self.mapping(os.getpid(), size=SIZE + 16384)
        with open_mapping(name) as memory:
            self.assertEqual(len(memory), SIZE)
        result = subprocess.run([str(self.reader), name], capture_output=True, text=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stdout+result.stderr)

    def test_readers_reject_undersized_mapping(self):
        # One byte short can round back up on macOS; stay below SIZE by a page.
        name = self.mapping(os.getpid(), size=SIZE - 16384, initialize=False)
        with self.assertRaises(ValueError):
            with open_mapping(name):
                pass
        result = subprocess.run([str(self.reader), name], capture_output=True, text=True, timeout=5)
        self.assertEqual(result.returncode, 2)

    def test_cpp_reader_decodes_stereo_tags_and_renews_lease(self):
        name = self.mapping(os.getpid())
        result = subprocess.run([str(self.reader), name], capture_output=True, text=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
        with open_mapping(name) as memory:
            self.assertGreater(struct.unpack_from("<Q", memory, 64)[0], 0)

    def test_cpp_reader_unlinks_a_dead_producer(self):
        with subprocess.Popen([sys.executable, "-c", "import time;time.sleep(20)"]) as producer:
            try:
                name = self.mapping(producer.pid)
                with subprocess.Popen([str(self.reader), name, "--watch-death"], stdout=subprocess.PIPE, text=True) as reader:
                    self.assertEqual(reader.stdout.readline().strip(), "opened")
                    producer.terminate()
                    producer.wait(timeout=3)
                    self.assertEqual(reader.wait(timeout=5), 0)
                with self.assertRaises(OSError):
                    with open_mapping(name):
                        pass
            finally:
                if producer.poll() is None:
                    producer.terminate()
                    producer.wait(timeout=3)


if __name__ == "__main__":
    unittest.main()

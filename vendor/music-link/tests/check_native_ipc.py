"""Headless native Rust-writer/C++-reader check; no audio SDKs needed."""
from pathlib import Path
import json
import os
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "music-link"))
from native_ipc import open_mapping, unlink_stopped_mapping, windows_mapping_exists


def msvc_generator():
    locator = Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")) / "Microsoft Visual Studio/Installer/vswhere.exe"
    installations = json.loads(subprocess.check_output(
        [str(locator), "-latest", "-products", "*", "-requires", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
         "-format", "json", "-utf8"], encoding="utf-8-sig"))
    if not installations:
        raise RuntimeError("Install Visual Studio C++ tools and the Windows SDK for the native IPC check")
    installation = installations[0]
    major = installation["installationVersion"].split(".")[0]
    capabilities = json.loads(subprocess.check_output(["cmake", "-E", "capabilities"], encoding="utf-8"))
    generator = next((g["name"] for g in capabilities["generators"]
                      if g["name"].startswith(f"Visual Studio {major} ")), None)
    if not generator:
        raise RuntimeError(f"Update CMake to support installed Visual Studio {major}")
    return ["-G", generator, "-A", "x64", "-DCMAKE_GENERATOR_INSTANCE=" + installation["installationPath"]]


def main():
    assert sys.platform in ("linux", "darwin", "win32"), "Unsupported native platform"
    fixture = ROOT / "music-link/tests/ipc-producer"
    env = dict(os.environ, RUSTUP_TOOLCHAIN="stable")
    subprocess.run(["cargo", "build", "--locked", "--manifest-path", str(fixture / "Cargo.toml")], env=env, check=True)
    producer = fixture / ("target/debug/lane-ipc-producer-check.exe" if os.name == "nt" else "target/debug/lane-ipc-producer-check")
    with tempfile.TemporaryDirectory(prefix="lane native ipc ") as temporary:
        if os.name == "nt":
            build = Path(temporary) / "build"
            # Select MSVC explicitly: a globally configured Ninja/Clang may not
            # have the Windows SDK environment supplied by a VS generator.
            subprocess.run(["cmake", "-S", str(ROOT / "music-link/tests/native-reader"), "-B", str(build),
                            *msvc_generator()], check=True)
            subprocess.run(["cmake", "--build", str(build), "--config", "Release"], check=True)
            reader = build / "Release/lane-ipc-reader.exe"
        else:
            reader = Path(temporary) / "reader"
            command = ["c++", "-std=c++17", "-pthread", "-I", str(ROOT / "vendor/BespokeSynth/files/Source"),
                       str(ROOT / "music-link/tests/ipc_reader.cpp"), "-o", str(reader)]
            if sys.platform == "linux":
                command += ["-lrt"]
            subprocess.run(command, check=True)
        for crash in (False, True):
            process = subprocess.Popen([str(producer)], stdout=subprocess.PIPE, text=True)
            name = process.stdout.readline().strip()
            if os.name == "nt":
                assert name.startswith("Local\\RustelAudio-"), name
            else:
                assert name.startswith("/rla-") and len(name) <= 31, name
            try:
                if crash and os.name == "nt":
                    # Existing readers retain their view after producer death;
                    # the kernel reclaims it after the final handle is closed.
                    with open_mapping(name) as memory:
                        process.kill()
                        process.wait(timeout=5)
                        assert windows_mapping_exists(name)
                        assert int.from_bytes(memory[:4], "little") == 0x524C4131
                elif crash:
                    watcher = subprocess.Popen([str(reader), name, "--watch-death"], stdout=subprocess.PIPE, text=True)
                    try:
                        assert watcher.stdout.readline().strip() == "opened"
                        process.kill()
                        process.wait(timeout=5)
                        assert watcher.wait(timeout=8) == 0
                    finally:
                        if watcher.poll() is None:
                            watcher.terminate()
                            watcher.wait(timeout=5)
                else:
                    subprocess.run([str(reader), name], check=True, timeout=8)
                    output = process.communicate(timeout=8)[0]
                    assert process.returncode == 0 and "muted=true restored=true" in output, output
                if os.name == "nt":
                    assert not windows_mapping_exists(name), "Windows mapping leaked"
                else:
                    try:
                        with open_mapping(name):
                            raise AssertionError("POSIX name was not cleaned up")
                    except OSError:
                        pass
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=5)
                unlink_stopped_mapping(name, process.pid)
    print("PASS Rust/C++ native IPC, monitor lease expiry, normal/crash mapping cleanup")


if __name__ == "__main__":
    main()

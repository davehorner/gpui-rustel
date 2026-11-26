#!/usr/bin/env python3
"""Build a pinned, clean upstream runtime without touching the patched submodule."""
import argparse
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent
REVISION = (ROOT / "upstream-revision.txt").read_text(encoding="utf-8").strip()

def run(*args):
    subprocess.run(args, check=True)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--debug", action="store_true")
    args = parser.parse_args()
    checkout = ROOT / "target" / "upstream-rustel"
    if not checkout.exists():
        checkout.parent.mkdir(parents=True, exist_ok=True)
        run("git", "clone", "--no-checkout", "https://github.com/tzfm/rustel.git", str(checkout))
    dirty = subprocess.check_output(["git", "-C", str(checkout), "status", "--porcelain"], text=True)
    if dirty.strip():
        raise SystemExit(f"Refusing to replace local changes in {checkout}")
    run("git", "-C", str(checkout), "fetch", "origin", REVISION)
    run("git", "-C", str(checkout), "checkout", "--detach", REVISION)
    command = ["cargo", "build", "--manifest-path", str(checkout / "Cargo.toml"),
               "-p", "rustel", "--no-default-features", "--features", "device-audio,midi", "--locked"]
    if not args.debug:
        command.append("--release")
    run(*command)
    import os
    print(checkout / "target" / ("debug" if args.debug else "release") / ("rustel.exe" if os.name == "nt" else "rustel"))

if __name__ == "__main__":
    main()

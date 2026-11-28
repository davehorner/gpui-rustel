#!/usr/bin/env python3
"""Build the pinned maintained engine used by Studio and browser audio."""
import argparse
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent

def run(*args):
    subprocess.run(args, check=True)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--debug", action="store_true")
    args = parser.parse_args()
    import sys
    sys.path.insert(0, str(ROOT.parent / "wasm-audio"))
    from prepare_engine import prepare
    checkout = prepare()
    command = ["cargo", "build", "--manifest-path", str(checkout / "Cargo.toml"),
               "-p", "rustel", "--no-default-features", "--features", "device-audio,midi"]
    if not args.debug:
        command.append("--release")
    run(*command)
    import os
    print(checkout / "target" / ("debug" if args.debug else "release") / ("rustel.exe" if os.name == "nt" else "rustel"))

if __name__ == "__main__":
    main()

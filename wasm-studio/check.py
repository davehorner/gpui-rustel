"""Compile-check upstream Studio against the browser target and QuickJS sysroot."""
from pathlib import Path
import json, os, shutil, subprocess, sys
HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
env = os.environ.copy()
metadata = json.loads(subprocess.check_output([
    'cargo', 'metadata', '--format-version=1', '--filter-platform', 'wasm32-unknown-unknown',
    '--manifest-path', str(HERE/'Cargo.toml'), '--locked'
], encoding='utf-8'))
if os.name == 'nt':
    headers = HERE/'target/wasm-libc-headers'
    quickjs = [p for p in metadata['packages'] if p['name'] == 'rquickjs-sys']
    for package in quickjs:
        source = Path(package['manifest_path']).parent/'vendor/wasi-libc/include'
        if source.exists(): shutil.copytree(source, headers, dirs_exist_ok=True)
    env['CFLAGS_wasm32_unknown_unknown'] = f'-I"{headers.as_posix()}"'
    env['CC_SHELL_ESCAPED_FLAGS'] = '1'
    env['CC_wasm32_unknown_unknown'] = shutil.which('clang') or 'clang'
    env['AR_wasm32_unknown_unknown'] = shutil.which('llvm-ar') or 'llvm-ar'
command = ['cargo', 'rustc' if '--build' in sys.argv else 'check', '--manifest-path', str(HERE/'Cargo.toml'), '--target', 'wasm32-unknown-unknown', '--lib', '--locked']
if '--build' in sys.argv: command += ['--release', '--', '-C', 'link-arg=-zstack-size=67108864']
subprocess.run(command, env=env, check=True)

"""Build the upstream Ratatui Studio browser frontend and versioned bindings."""
from pathlib import Path
import argparse, hashlib, os, shutil, subprocess, sys, tomllib
HERE=Path(__file__).resolve().parent
ROOT=HERE.parent
sys.path.insert(0,str(ROOT/'wasm-audio'))
from prepare_engine import prepare

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',type=Path,default=ROOT/'target/gpui-rustel-web/studio')
    parser.add_argument('--target',choices=['web','nodejs'],default='web')
    args=parser.parse_args()
    prepare()
    subprocess.run([sys.executable,str(HERE/'check.py'),'--build'],check=True,cwd=ROOT)
    binary=HERE/'target/wasm32-unknown-unknown/release/rustel_browser_studio.wasm'
    version=hashlib.sha256(binary.read_bytes()+(ROOT/'web/studio.js').read_bytes()).hexdigest()[:16]
    lock=tomllib.loads((HERE/'Cargo.lock').read_text(encoding='utf-8'))
    bindgen=next(p['version'] for p in lock['package'] if p['name']=='wasm-bindgen')
    local=ROOT/'target/wasm-tools/bin'/('wasm-bindgen.exe' if os.name=='nt' else 'wasm-bindgen')
    tool=str(local) if local.exists() else shutil.which('wasm-bindgen')
    if not tool or subprocess.check_output([tool,'--version'],text=True).strip()!=f'wasm-bindgen {bindgen}':raise SystemExit(f'Install wasm-bindgen-cli {bindgen}')
    subprocess.run([tool,str(binary),'--target',args.target,'--out-dir',str(args.out/version),'--no-typescript'],check=True)
    (args.out/'version.txt').write_text(version,encoding='utf-8')
    print(f'Studio {version}')
if __name__=='__main__':main()

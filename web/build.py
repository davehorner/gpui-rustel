"""Build the shared GPUI application for the browser and generate its JS bindings."""
from pathlib import Path
import argparse,os,shutil,subprocess,tomllib,json,hashlib,sys
ROOT=Path(__file__).resolve().parents[1]
APP=ROOT
TOOLCHAIN="nightly-2026-10-31"
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out",type=Path,default=ROOT/"target/gpui-rustel-web")
    args=parser.parse_args()
    args.out.mkdir(parents=True,exist_ok=True)
    subprocess.run(["cargo",f"+{TOOLCHAIN}","rustc","--manifest-path",str(APP/"Cargo.toml"),"--target-dir",str(ROOT/"target"),"--lib","--target","wasm32-unknown-unknown","--profile","web","--locked","--","-C","link-arg=-zstack-size=8388608"],check=True,cwd=ROOT)
    lock=tomllib.loads((APP/"Cargo.lock").read_text(encoding="utf-8"))
    version=next(p["version"] for p in lock["package"] if p["name"]=="wasm-bindgen")
    local=ROOT/"target/wasm-tools/bin"/("wasm-bindgen.exe" if os.name=="nt" else "wasm-bindgen")
    tool=str(local) if local.exists() else shutil.which("wasm-bindgen")
    if not tool or subprocess.check_output([tool,"--version"],text=True).strip()!=f"wasm-bindgen {version}":
        raise SystemExit(f"Install the matching project tool: cargo install wasm-bindgen-cli --version {version} --locked --root target/wasm-tools")
    audio=APP/"wasm-audio"
    subprocess.run([sys.executable,str(audio/"build.py"),"--out",str(args.out/"audio")],check=True,cwd=ROOT)
    audio_hash=hashlib.sha256()
    for file in [APP/'web'/name for name in ['audio-host.js','audio-worker.js','audio-worklet.js','sample-library.js']]+[args.out/'audio/rustel_browser_audio.js',args.out/'audio/rustel_browser_audio_bg.wasm']:
        audio_hash.update(file.read_bytes())
    host_version=audio_hash.hexdigest()[:16]
    viewer_wasm=ROOT/"target/wasm32-unknown-unknown/web/gpui_rustel_app.wasm"
    viewer_version=hashlib.sha256(viewer_wasm.read_bytes()+(APP/"web/startup.js").read_bytes()+(APP/"web/index.html").read_bytes()+host_version.encode()).hexdigest()[:16]
    subprocess.run([tool,str(viewer_wasm),"--target","web","--out-dir",str(args.out/"pkg"/viewer_version),"--no-typescript","--out-name","gpui_rustel"],check=True)
    scores=args.out/"scores";scores.mkdir(exist_ok=True)
    catalog=[]
    for file in sorted((APP/"demos").glob("*.strudel")):
        content=file.read_bytes();(scores/file.name).write_bytes(content)
        catalog.append({"name":file.name,"url":"./scores/"+file.name,"sha256":hashlib.sha256(content).hexdigest()})
    banks=args.out/"banks";banks.mkdir(exist_ok=True)
    for name in ["sample-banks.json","gm-fonts.json"]:
        shutil.copyfile(ROOT/".build-sources/rustel-browser-engine/crates/runtime/assets"/name,banks/name)
    for file in (APP/"web").iterdir():
        if file.suffix==".html":
            (args.out/file.name).write_text(file.read_text(encoding="utf-8").replace("__AUDIO_HOST_VERSION__",host_version).replace("__VIEWER_VERSION__",viewer_version),encoding="utf-8",newline="\n")
        elif file.suffix in {".js",".json"}:shutil.copyfile(file,args.out/file.name)
    (args.out/"javascript-demos.json").write_text(json.dumps(catalog,indent=2)+"\n",encoding="utf-8")
    (args.out/".nojekyll").touch()
    licenses=args.out/"licenses";licenses.mkdir(exist_ok=True)
    shutil.copyfile(APP/"assets/README.md",licenses/"Fox-attribution.md")
    shutil.copyfile(audio/"LICENSE",licenses/"Rustel-AGPL-3.0.txt")
    (licenses/"SOURCE.txt").write_text("Rustel engine: https://github.com/tzfm/rustel/tree/79fd1fb3fd64c0a637f475ad24de675fd5b2ac59\nBrowser host source: https://github.com/davehorner/gpui-rustel (wasm-audio and web folders).\n",encoding="utf-8")
    source=args.out/"source/rustel-browser-audio";(source/"src").mkdir(parents=True,exist_ok=True)
    for name in ["Cargo.toml","Cargo.lock","LICENSE","README.md","prepare_engine.py","build.py"]:shutil.copyfile(audio/name,source/name)
    shutil.copytree(audio/"src",source/"src",dirs_exist_ok=True)
    shutil.copytree(audio/"patches",source/"patches",dirs_exist_ok=True)
    print(f"Built {args.out}\nServe: python gpui-rustel/web/serve.py --directory {args.out}")
if __name__=="__main__":main()

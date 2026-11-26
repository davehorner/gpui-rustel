"""Build the JavaScript-enabled Rustel WASM engine and bindings."""
from pathlib import Path
import argparse,json,os,shutil,subprocess,tomllib
from prepare_engine import prepare,HERE,ROOT

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out",type=Path,default=ROOT/"target/gpui-rustel-web/audio")
    parser.add_argument("--target",choices=["web","nodejs"],default="web")
    args=parser.parse_args();prepare()
    env=os.environ.copy()
    metadata=json.loads(subprocess.check_output(["cargo","metadata","--format-version=1","--manifest-path",str(HERE/"Cargo.toml"),"--locked"],text=True,encoding="utf-8"))
    quickjs=next(p for p in metadata["packages"] if p["name"]=="rquickjs-sys")
    quickjs_root=Path(quickjs["manifest_path"]).parent
    if os.name=="nt":
        headers=quickjs_root/"vendor/wasi-libc/include"
        copied=HERE/"target/wasm-libc-headers";shutil.copytree(headers,copied,dirs_exist_ok=True)
        # Clang's nested includes fail under canonical Windows \\?\ paths.
        env["CFLAGS_wasm32_unknown_unknown"]=env.get("CFLAGS_wasm32_unknown_unknown","")+f' -I"{copied.as_posix()}"'
        env["CC_SHELL_ESCAPED_FLAGS"]="1"
        env.setdefault("CC_wasm32_unknown_unknown",shutil.which("clang") or "clang")
        env.setdefault("AR_wasm32_unknown_unknown",shutil.which("llvm-ar") or "llvm-ar")
    subprocess.run(["cargo","rustc","--manifest-path",str(HERE/"Cargo.toml"),"--lib","--target","wasm32-unknown-unknown","--release","--locked","--","-C","link-arg=-zstack-size=67108864"],check=True,env=env,cwd=ROOT)
    version=next(p["version"] for p in tomllib.loads((HERE/"Cargo.lock").read_text(encoding="utf-8"))["package"] if p["name"]=="wasm-bindgen")
    tool=ROOT/"target/wasm-tools/bin"/("wasm-bindgen.exe" if os.name=="nt" else "wasm-bindgen")
    command=str(tool) if tool.exists() else shutil.which("wasm-bindgen")
    if not command or subprocess.check_output([command,"--version"],text=True).strip()!=f"wasm-bindgen {version}":raise SystemExit(f"Install wasm-bindgen-cli {version} into target/wasm-tools")
    subprocess.run([command,str(HERE/"target/wasm32-unknown-unknown/release/rustel_browser_audio.wasm"),"--target",args.target,"--out-dir",str(args.out),"--no-typescript"],check=True)
    licenses=args.out/"licenses";licenses.mkdir(parents=True,exist_ok=True)
    for source,name in [(quickjs_root.parent/"LICENSE","rquickjs.txt"),(quickjs_root/"quickjs/LICENSE","QuickJS.txt"),(quickjs_root/"vendor/wasi-libc/NOTICE.md","wasi-libc.md")]:
        shutil.copyfile(source,licenses/name)
if __name__=="__main__":main()

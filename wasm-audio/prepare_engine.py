"""Prepare a pinned, isolated Rustel checkout with the browser portability patch."""
from pathlib import Path
import subprocess
HERE=Path(__file__).resolve().parent
ROOT=HERE.parent
REVISION="79fd1fb3fd64c0a637f475ad24de675fd5b2ac59"
CHECKOUT=ROOT/"target/rustel-browser-engine"
def prepare():
    if not CHECKOUT.exists():
        CHECKOUT.parent.mkdir(parents=True,exist_ok=True)
        local=ROOT/"target/rustel-upstream"
        origin=str(local) if (local/".git").is_dir() else "https://github.com/tzfm/rustel.git"
        subprocess.run(["git","clone","--no-checkout",origin,str(CHECKOUT)],check=True)
        subprocess.run(["git","-C",str(CHECKOUT),"checkout","--detach",REVISION],check=True)
    head=subprocess.check_output(["git","-C",str(CHECKOUT),"rev-parse","HEAD"],text=True).strip()
    if head!=REVISION:raise SystemExit(f"Unexpected browser engine revision: {head}")
    patch=HERE/"patches/browser-portability.patch"
    changes=subprocess.check_output(["git","-C",str(CHECKOUT),"diff","--","crates"])
    if not changes:subprocess.run(["git","-C",str(CHECKOUT),"apply",str(patch)],check=True)
    elif changes.replace(b"\r\n",b"\n")!=patch.read_bytes().replace(b"\r\n",b"\n"):
        raise SystemExit(f"Unrecognized local edits in {CHECKOUT}; inspect them before building")
    return CHECKOUT
if __name__=="__main__":print(prepare())

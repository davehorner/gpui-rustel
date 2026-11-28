"""Initialize pinned dependencies and the maintained Rustel viewer extensions."""
from pathlib import Path
import subprocess,sys
ROOT=Path(__file__).resolve().parents[1]
subprocess.run(['git','submodule','update','--init','--depth','1','--','vendor/gpui-nightshade'],cwd=ROOT,check=True)
engine=ROOT/'vendor/gpui-nightshade/upstream/nightshade'
patches=[ROOT/'patches'/name for name in ['viewer-generated-scene.patch','viewer-music-extension.patch']]
# Remove only our verified patches before the renderer checks its own maintained
# integration diff; then apply them again in dependency order.
if (engine/'.git').exists():
    for patch in reversed(patches):
        if subprocess.run(['git','apply','--reverse','--check',str(patch)],cwd=engine,capture_output=True).returncode==0:
            subprocess.run(['git','apply','--reverse',str(patch)],cwd=engine,check=True)
subprocess.run([sys.executable,str(ROOT/'vendor/gpui-nightshade/scripts/prepare.py')],check=True)
for patch in patches:
    subprocess.run(['git','apply','--check',str(patch)],cwd=engine,check=True)
    subprocess.run(['git','apply',str(patch)],cwd=engine,check=True)

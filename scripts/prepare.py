"""Initialize the pinned Nightshade integration and its component/renderer dependencies."""
from pathlib import Path
import subprocess,sys
ROOT=Path(__file__).resolve().parents[1]
subprocess.run(['git','submodule','update','--init','--depth','1','--','vendor/gpui-nightshade'],cwd=ROOT,check=True)
subprocess.run([sys.executable,str(ROOT/'vendor/gpui-nightshade/scripts/prepare.py')],check=True)

# Register Rustel's procedural scene with the pinned viewer's native MCP tools.
engine=ROOT/'vendor/gpui-nightshade/upstream/nightshade'
patch=ROOT/'patches/viewer-generated-scene.patch'
if subprocess.run(['git','apply','--reverse','--check',str(patch)],cwd=engine,capture_output=True).returncode:
    subprocess.run(['git','apply','--check',str(patch)],cwd=engine,check=True)
    subprocess.run(['git','apply',str(patch)],cwd=engine,check=True)

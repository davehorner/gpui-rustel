"""Initialize the pinned Nightshade integration and its component/renderer dependencies."""
from pathlib import Path
import subprocess,sys
ROOT=Path(__file__).resolve().parents[1]
subprocess.run(['git','submodule','update','--init','--depth','1','--','vendor/gpui-nightshade'],cwd=ROOT,check=True)
subprocess.run([sys.executable,str(ROOT/'vendor/gpui-nightshade/scripts/prepare.py')],check=True)

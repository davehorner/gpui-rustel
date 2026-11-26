"""Serve the built GPUI viewer locally with the headers required by browser graphics."""
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler,ThreadingHTTPServer
from pathlib import Path
class Handler(SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cross-Origin-Opener-Policy","same-origin")
        self.send_header("Cross-Origin-Embedder-Policy","require-corp")
        self.send_header("Cache-Control","no-cache")
        super().end_headers()
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory",type=Path,default=Path(__file__).resolve().parents[1]/"target/gpui-rustel-web")
    parser.add_argument("--port",type=int,default=18766)
    args=parser.parse_args()
    if not (args.directory/"index.html").is_file():raise SystemExit("Build the web viewer first: python gpui-rustel/web/build.py")
    server=ThreadingHTTPServer(("127.0.0.1",args.port),partial(Handler,directory=str(args.directory.resolve())))
    print(f"GPUI Rustel: http://127.0.0.1:{args.port}/",flush=True)
    try:server.serve_forever()
    except KeyboardInterrupt:pass
    finally:server.server_close()
if __name__=="__main__":main()

"""Check live demo add/rename/remove notifications without changing playback."""
import json,sys,time,urllib.request,uuid
from pathlib import Path
url=sys.argv[1] if len(sys.argv)>1 else 'http://127.0.0.1:9620'
def state():
    body=json.dumps(dict(jsonrpc='2.0',id=1,method='tools/call',params=dict(name='get_music_state',arguments={}))).encode()
    with urllib.request.urlopen(urllib.request.Request(url,data=body,headers={'Content-Type':'application/json'}),timeout=25) as response:r=json.load(response)['result']
    assert not r.get('isError'),r
    return r['structuredContent']
def wait(predicate):
    deadline=time.monotonic()+10
    while time.monotonic()<deadline:
        try:
            current=state()
            if predicate(current):return current
        except (OSError,KeyError,AssertionError):pass
        time.sleep(.1)
    raise AssertionError('Demo notification was not reflected in music state')
initial=wait(lambda s:bool(s.get('session')))
directory=Path(__file__).resolve().parents[1]/'demos'
name='Watch probe '+uuid.uuid4().hex
first=directory/(name+'.strudel');second=directory/(name+' renamed.strudel')
try:
    first.write_text("note('c4').s('sine').gain(.01)",encoding='utf-8')
    added=wait(lambda s:first.name in s['demos'])
    first.rename(second)
    renamed=wait(lambda s:first.name not in s['demos'] and second.name in s['demos'])
    second.unlink()
    removed=wait(lambda s:second.name not in s['demos'])
    assert all(s['generation']==initial['generation'] for s in [added,renamed,removed]),'Catalogue update changed playback generation'
    print(json.dumps(dict(added=True,renamed=True,removed=True,generation_unchanged=True,demo=removed['demo'],bachatas=[n for n in removed['demos'] if 'Bachata' in n])))
finally:
    for path in [first,second]:
        if path.exists():path.unlink()

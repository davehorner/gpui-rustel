"""Execute a saved demo through MCP and verify its selected catalogue name."""
import json,sys,time,urllib.request
from pathlib import Path
name=sys.argv[1] if len(sys.argv)>1 else 'Bachata luna de cobre.strudel'
score=(Path(__file__).resolve().parents[1]/'demos'/name).read_text(encoding='utf-8')
def call(name,args={}):
    data=json.dumps(dict(jsonrpc='2.0',id=1,method='tools/call',params=dict(name=name,arguments=args))).encode()
    with urllib.request.urlopen(urllib.request.Request('http://127.0.0.1:9620',data=data,headers={'Content-Type':'application/json'}),timeout=25) as response:r=json.load(response)['result']
    assert not r.get('isError'),r
    return r['structuredContent']
state=call('get_music_state');assert name in state['demos']
answer=call('execute_music_score',dict(session=state['session'],generation=state['generation'],source=score));assert answer['acknowledged']
time.sleep(.5)
state=call('get_music_state');assert state['demo']==name,state['demo']
assert call('get_music_score')['source']==score
print(json.dumps(dict(selected_demo=state['demo'],exact_source=True,playing=not state['transport']['paused'])))

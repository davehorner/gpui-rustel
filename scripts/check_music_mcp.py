"""Exercise music MCP against an owned desktop runtime; restores its initial demo."""
import json,sys,time,urllib.request
url=sys.argv[1] if len(sys.argv)>1 else 'http://127.0.0.1:9620'
serial=0
def call(name,arguments=None,error=False):
    global serial
    serial+=1
    body=json.dumps(dict(jsonrpc='2.0',id=serial,method='tools/call',params=dict(name=name,arguments=arguments or {}))).encode()
    with urllib.request.urlopen(urllib.request.Request(url,data=body,headers={'Content-Type':'application/json'}),timeout=25) as response:reply=json.load(response)
    assert 'error' not in reply,reply
    result=reply['result'];assert bool(result.get('isError'))==error,result
    return result.get('structuredContent') or json.loads(result['content'][0]['text'])
def state():
    value=call('get_music_state')
    assert value.get('session'),value
    return value
def mutate(name,fields,error=False):
    current=state()
    return call(name,dict(session=current['session'],generation=current['generation'],**fields),error)
for attempt in range(60):
    try:
        initial=state();break
    except (AssertionError,OSError):
        if attempt==59:raise
        time.sleep(.5)
original=call('get_music_score');demo=initial['demo']
call('set_music_transport',dict(session=initial['session'],generation=initial['generation']+999,action='stop'),True)
mutate('set_music_transport',dict(action='pause'),True)
assert mutate('set_music_transport',dict(action='stop'))['acknowledged']
assert mutate('set_music_transport',dict(action='play'))['acknowledged']
time.sleep(.5)
assert mutate('execute_music_score',dict(source="console.log('MCP editor test'); note('c4').s('sine').gain(.03)"))['acknowledged']
time.sleep(.5)
edited=call('get_music_score');assert "MCP editor test" in edited['source'],edited
mutate('execute_music_score',dict(source='const ='),True)
assert call('get_music_score')['source']==edited['source'],'Rejected edit replaced the running source'
messages=call('get_music_messages');assert any('MCP editor test' in message for message in messages['messages']),messages
if demo in initial['demos']:
    assert mutate('select_music_demo',dict(demo=demo))['acknowledged']
    time.sleep(.5)
    assert call('get_music_score')['source']==original['source']
print(json.dumps(dict(music_state=True,stale_rejected=True,unsupported_pause_rejected=True,stop_play=True,execute=True,invalid_edit_preserved=True,messages=len(messages['messages']),restored_demo=demo)))

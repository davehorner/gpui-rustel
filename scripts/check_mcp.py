"""Smoke-test a running desktop Rustel scene MCP endpoint (stdlib only)."""
import json,sys,time,urllib.request
url=sys.argv[1] if len(sys.argv)>1 else 'http://127.0.0.1:9620'
serial=0
def call(method,params):
    global serial
    serial+=1
    body=json.dumps(dict(jsonrpc='2.0',id=serial,method=method,params=params)).encode()
    with urllib.request.urlopen(urllib.request.Request(url,data=body,headers={'Content-Type':'application/json'}),timeout=25) as response:
        result=json.load(response)
    assert 'error' not in result,result
    return result['result']
def tool(name,arguments):
    result=call('tools/call',dict(name=name,arguments=arguments))
    assert not result.get('isError'),result
    return result.get('structuredContent') or json.loads(result['content'][0]['text'])
init=call('initialize',dict(protocolVersion='2024-11-05',capabilities={},clientInfo=dict(name='rustel-check',version='1')))
tools=call('tools/list',{})['tools']
assert any(tool['name']=='get_scene' for tool in tools)
for attempt in range(20):
    scene=tool('get_scene',{})
    if scene['nodes']:break
    time.sleep(.5)
assert scene['nodes'],'No procedural scene nodes were registered'
entity=scene['nodes'][0]['entity']
transform=tool('get_transform',dict(entity=entity))
assert transform,transform
print(json.dumps(dict(server=init['serverInfo'],tools=len(tools),scene_nodes=len(scene['nodes']),queried_entity=entity)))

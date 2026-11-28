"""Exercise Studio MCP against a dedicated desktop session; restore its score/draft.

Usage: python scripts/check_studio_mcp.py --url http://127.0.0.1:9632
This intentionally edits the Studio draft and temporarily stops/executes music.
"""
import argparse,json,time,urllib.request
from concurrent.futures import ThreadPoolExecutor

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--url',default='http://127.0.0.1:9620')
args=parser.parse_args()
def rpc(method,params):
    body=json.dumps(dict(jsonrpc='2.0',id=1,method=method,params=params)).encode()
    with urllib.request.urlopen(urllib.request.Request(args.url,data=body,headers={'Content-Type':'application/json'}),timeout=30) as response:reply=json.load(response)
    if 'error' in reply:raise RuntimeError(reply['error'])
    return reply['result']
def tool(name,**arguments):
    result=rpc('tools/call',dict(name=name,arguments=arguments))
    value=result.get('structuredContent')
    if value is None:value=json.loads(result['content'][0]['text'])
    if result.get('isError'):raise RuntimeError(value)
    return value
def guard(draft):return dict(draft_id=draft['draft_id'],expected_revision=draft['revision'])
def control(name,**arguments):
    for attempt in range(4):
        state=tool('get_music_state')
        try:return tool(name,session=state['session'],generation=state['generation'],**arguments)
        except RuntimeError as error:
            if 'stale' not in str(error).lower() or attempt==3:raise
            time.sleep(.1)
def refused(name,**arguments):
    try:tool(name,**arguments)
    except RuntimeError as error:return str(error)
    raise AssertionError(f'{name} unexpectedly succeeded')
catalog=rpc('tools/list',{})['tools']
expected={'get_studio_draft','set_studio_draft','validate_studio_draft','execute_studio_draft'}
assert expected.issubset({t['name'] for t in catalog})
original=tool('get_music_score');initial_state=tool('get_music_state');initial_draft=tool('get_studio_draft')
checks=[]
try:
    control('set_music_transport',action='stop')
    for _ in range(30):
        stopped=tool('get_music_state')
        if stopped['transport']['paused']:break
        time.sleep(.1)
    assert stopped['transport']['paused']
    generation=stopped['generation']
    draft=tool('get_studio_draft')
    invalid=tool('set_studio_draft',**guard(draft),source='const broken = (')
    assert invalid['source']=='const broken = (' and invalid['dirty'] and not invalid['playback_changed']
    assert 'Stale Studio draft' in refused('set_studio_draft',**guard(draft),source='would overwrite newer work')
    report=tool('validate_studio_draft',**guard(invalid))
    assert report['valid'] is False and report['diagnostics'],report
    assert tool('get_music_score')==original
    state=tool('get_music_state');assert state['generation']==generation and state['transport']['paused']
    checks.append('invalid draft diagnostics without changing score, generation or stopped transport')
    error=refused('execute_studio_draft',**guard(invalid),session=state['session'],generation=state['generation'])
    assert 'rejected' in error.lower(),error
    assert tool('get_music_score')==original
    checks.append('invalid execution preserves the existing runtime score')
    # Two editors cannot both replace the same observed revision.
    latest=tool('get_studio_draft')
    source='$: note("d3 f3 a3").s("sine").gain(0.05)._pianoroll()\n'
    def compete(_):
        try:return tool('set_studio_draft',**guard(latest),source=source)
        except RuntimeError as error:return str(error)
    with ThreadPoolExecutor(max_workers=2) as pool:results=list(pool.map(compete,range(2)))
    assert sum(isinstance(r,dict) for r in results)==1,results
    checks.append('concurrent edits reject an outdated revision')
    draft=tool('get_studio_draft');report=tool('validate_studio_draft',**guard(draft))
    assert report['valid'] is True,report
    state=tool('get_music_state');assert state['generation']==generation and state['transport']['paused']
    assert tool('get_music_score')==original
    checks.append('valid draft validation does not start playback')
    assert 'Stale music' in refused('execute_studio_draft',**guard(draft),session=state['session'],generation=0)
    assert 'Stale Studio' in refused('execute_studio_draft',**guard(invalid),session=state['session'],generation=state['generation'])
    result=tool('execute_studio_draft',**guard(draft),session=state['session'],generation=state['generation'])
    assert result['acknowledged'] and result['boundary']=='runtime_score_snapshot',result
    score=tool('get_music_score');assert score['source']==source and score['session']==original['session']
    state=tool('get_music_state');assert state['generation']!=generation and not state['transport']['paused']
    assert not tool('get_studio_draft')['dirty']
    checks.append('exact draft execution acknowledged in the same runtime session')
    playing_generation=state['generation'];draft=tool('get_studio_draft')
    assert tool('validate_studio_draft',**guard(draft))['valid']
    state=tool('get_music_state')
    assert state['generation']==playing_generation and not state['transport']['paused']
    checks.append('validation also preserves an already-playing session')
finally:
    # Do not silently overwrite a concurrent user's draft during cleanup.
    current=tool('get_studio_draft')
    if current['execution_pending']:
        raise RuntimeError('Execution outcome is pending; inspect music state before restoring')
    if current['source'] not in {initial_draft['source'],'const broken = (',locals().get('source','')}:
        raise RuntimeError('Studio draft changed outside this test; left it untouched')
    control('execute_music_score',source=original['source'])
    current=tool('get_studio_draft')
    tool('set_studio_draft',**guard(current),source=initial_draft['source'])
    if initial_state['transport']['paused']:control('set_music_transport',action='stop')
print(json.dumps(dict(passed=len(checks),checks=checks,original_score_and_draft_restored=True),indent=2))

# Music MCP

Music MCP exposes the running desktop Rustel audio session through the same localhost endpoint as the Nightshade scene tools. It uses the normal music command bridge and waits for correlated runtime responses. Scene animation tools such as `set_playback` do **not** control the music; use `set_music_transport`.

## Start and connect

From the repository root, initialize dependencies and build the native runtime:

```powershell
python scripts/prepare.py
python upstream-adapter/build_runtime.py
```

Launch with the absolute runtime executable path printed by the runtime build, and an existing score:

```powershell
cargo run --locked -- --rustel="C:\path\to\rustel.exe" --score="demos/Bachata.strudel" --agent-port=9620
```

The default endpoint is **http://127.0.0.1:9620**. `--agent-port=PORT` changes it; `--no-agent` disables both scene and music tools. The endpoint binds only to loopback. A startup line saying the endpoint was requested is not confirmation of a successful bind. Allow the stage to initialize, then use `tools/list` to confirm readiness and discover the actual schemas.

This is the viewer's existing **POST JSON-RPC** transport, advertising protocol version `2024-11-05`. It is not stdio or a complete modern Streamable HTTP transport; clients that require streaming/session negotiation need a compatible adapter. There is no separate music server or second port.

The browser GPUI/WASM and `?studio` applications do not host this native HTTP endpoint. Their editors can evaluate locally, but this endpoint controls the desktop process.

## Native Studio drafts

The same endpoint also exposes four tools for the real Studio editor inside GPUI. Requests are serviced on the UI thread, including while the Studio pane is closed. Reading a draft initializes the editor if necessary but does not open the pane or play audio.

| Tool | Required arguments | Result |
|---|---|---|
| `get_studio_draft` | None | Selected scene's exact source, `draft_id`, monotonic `revision`, dirty/open state, and running session/generation |
| `set_studio_draft` | `draft_id`, `expected_revision`, `source` | One undoable replacement; playback stays unchanged |
| `validate_studio_draft` | `draft_id`, `expected_revision` | `valid`, structured Rustel diagnostics and the captured revision; does not install the score or start/stop audio |
| `execute_studio_draft` | `draft_id`, `expected_revision`, `session`, `generation` | Executes that exact draft in the existing owned runtime and waits for a correlated acknowledgement at `runtime_score_snapshot` |

Draft text and evaluated score are distinct. dirty means the draft differs from the evaluated score; it is not a disk-save flag. `get_music_score` still reads the running score; it does not read unsent Studio edits. Always read `get_studio_draft` before editing. Both the draft ID and revision are checked on the UI thread, so stale clients cannot overwrite newer edits. Undo, redo, and scene changes invalidate observed revisions. Sources are bounded to **512 KiB of UTF-8**, not just a character count. In-progress text composition or execution can refuse a replacement rather than losing user input.

Validation uses the current owned runtime's executable in its validation-only mode, with a private sibling file to retain the current score's relative asset base. It runs off the GPUI thread, opens no playback device and sends no transport or score-install command. Only one validation-only request runs at a time, with an eight-second subprocess deadline and temporary-file cleanup. A validation result applies to its returned revision; it is not a promise that a subsequently edited draft is valid. Execution validates again before replacing playback, so an invalid draft preserves the previous score. Validation/execution require an owned runtime; a hub-only connection can still read and edit Studio drafts.

Example, using the `tool()` helper below:

```python
draft = tool("get_studio_draft")
draft = tool("set_studio_draft",
    draft_id=draft["draft_id"], expected_revision=draft["revision"],
    source='$: note("c3 e3 g3").s("sine").gain(0.1)._pianoroll()')
guard = {"draft_id": draft["draft_id"], "expected_revision": draft["revision"]}
report = tool("validate_studio_draft", **guard)
if report["valid"]:
    state = tool("get_music_state")
    result = tool("execute_studio_draft", **guard,
        session=state["session"], generation=state["generation"])
else:
    print(report["diagnostics"])
```

The UI request queue is bounded. Requests that expire before the UI starts them are discarded. An execution timeout after submission has an **unknown outcome**: read both the draft and music state before deciding what to do next; do not automatically resend it. Execution may be refused while another Studio evaluation is pending. Hidden/minimized panes still service requests through the app's existing UI update turn; no filesystem polling is added.

For a dedicated test session, `python scripts/check_studio_mcp.py --url http://127.0.0.1:9632` checks revision protection, invalid diagnostics, validation without playback, and acknowledged execution. It temporarily edits drafts and stops/executes music, then restores the original source and draft. Do not run it against a session somebody else is editing.

### Read-only connection check

```powershell
Invoke-RestMethod http://127.0.0.1:9620 -Method Post -ContentType application/json -Body '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

A dependency-free Python client follows. Save it in a local script, or run the block interactively. The initial calls only read state.

```python
import itertools
import json
import urllib.request

URL = "http://127.0.0.1:9620"
ids = itertools.count(1)

def rpc(method, params):
    body = json.dumps({
        "jsonrpc": "2.0", "id": next(ids),
        "method": method, "params": params,
    }).encode("utf-8")
    request = urllib.request.Request(
        URL, data=body, headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=25) as response:
        reply = json.load(response)
    if "error" in reply:
        raise RuntimeError(reply["error"])
    return reply["result"]

def tool(name, **arguments):
    result = rpc("tools/call", {"name": name, "arguments": arguments})
    value = result.get("structuredContent")
    if value is None:
        value = json.loads(result["content"][0]["text"])
    if result.get("isError"):
        raise RuntimeError(value)
    return value

rpc("initialize", {
    "protocolVersion": "2024-11-05", "capabilities": {},
    "clientInfo": {"name": "music-example", "version": "1"},
})
state = tool("get_music_state")
print(state)
```

If exactly one session exists, reads can omit `session`. Otherwise `get_music_state` returns the available session IDs and status; choose one and call `tool("get_music_state", session="<returned ID>")`. With no live session, check the runtime messages and launch arguments. Music tools become available after stage initialization.

## Tools

`tools/list` is authoritative. Unknown arguments, missing required values and null required values are rejected. Every mutation requires `session` and integer `generation` from a fresh state response, in addition to the arguments below.

| Tool | Additional arguments | Result / behavior |
| --- | --- | --- |
| `get_music_state` | Optional `session` | Sessions, selected session/generation, display cycle, tempo, transport, active demo, demo names, real lanes, capabilities, status/message and `fresh`. Without a unique session, returns session IDs and status instead. |
| `get_music_score` | Optional `session` | Exact evaluated `source`, session, generation and `fresh`. Does not save a file. |
| `get_music_messages` | None | Latest 200 runtime messages/control results, plus status and feedback. The buffer is bounded and is not durable history. |
| `set_music_transport` | `action`: `play`, `stop`, `pause` or `seek`; `cycle` required only for seek | Controls audio transport. Hosted native Play restarts the owned runtime. Pause/seek require advertised backend support. Seek cycle range: 0–1,000,000. |
| `set_music_tempo` | `bpm`: 1–1000; integer `top`: 1–64; `bottom`: 1, 2, 4, 8, 16, 32 or 64 | Sets tempo and time signature through the runtime control interface. |
| `set_music_lane_transpose` | `lane`; integer `semitones`: −48–48 | Sets absolute transpose for a real emitting lane. |
| `set_music_lane_solo` | `lane`; boolean `enabled` | Enables/disables lane audition. Disabling clears solo. |
| `select_music_demo` | `demo`: an exact name from `state["demos"]` | Loads a catalogued score; do not pass an arbitrary path. |
| `execute_music_score` | `source`: score text, at most 512 KiB of UTF-8 | Validates and executes text in an **owned** runtime. Preserves the original score file. Unavailable for external hub producers. |

Use the lane IDs returned by `get_music_state` (for example `#0`). Expanded const/instrument lanes in the visualizer are projections, not independent runtime control targets.

`tempo` can be null if no explicit tempo record/control has populated it; that does not imply the score has no tempo. `cycle` is the viewer's display position, not proof of audible playback. Messages belong to the application's shared runtime/control log, not a session-filtered log.

## Control examples

Continue using the Python helper above. Read state immediately before a mutation:

```python
SESSION = state.get("session")  # Choose a returned session ID if there are several.
if SESSION is None:
    raise RuntimeError("Choose a live session first")

def mutate(name, **arguments):
    current = tool("get_music_state", session=SESSION)
    if not current["fresh"]:
        raise RuntimeError("Wait for fresh runtime state")
    return tool(name, session=SESSION,
                generation=current["generation"], **arguments)
```

These examples change the running music; run the desired calls individually:

```python
mutate("set_music_transport", action="stop")
mutate("set_music_transport", action="play")  # Restart on the hosted native backend.
mutate("set_music_tempo", bpm=124, top=4, bottom=4)

current = tool("get_music_state", session=SESSION)
lane = current["lanes"][0]["id"]  # Requires a nonempty lane list.
mutate("set_music_lane_transpose", lane=lane, semitones=2)
mutate("set_music_lane_solo", lane=lane, enabled=True)
mutate("set_music_lane_solo", lane=lane, enabled=False)
mutate("set_music_lane_transpose", lane=lane, semitones=0)
```

Choose a saved demo using its exact catalogue name:

```python
current = tool("get_music_state", session=SESSION)
name = "Bachata original con conteo.strudel"
if name not in current["demos"]:
    raise RuntimeError("This demo is not in the runtime catalogue")
mutate("select_music_demo", demo=name)
```

Execute new code without overwriting the original file:

```python
source = "setcpm(120/4)\n$: note(\"c4 e4 g4 e4\").s(\"sine\").gain(.08)\n"
answer = mutate("execute_music_score", source=source)
print(answer)
print(tool("get_music_score", session=SESSION))
print(tool("get_music_messages"))
```

For an existing local demo, preserve its contents when sending it:

```python
from pathlib import Path
source = Path("demos/Salsa luz de tarde.strudel").read_text(encoding="utf-8")
mutate("execute_music_score", source=source)
```

The desktop editor follows a successfully installed source. If that replaces unsaved text, **Restore previous draft** recovers the displaced buffer. If source text matches a saved demo, allowing CRLF/LF line-ending differences, the dropdown selects that demo's name. Unsaved compositions display **Editor**.

Saving new `.strudel` files in the configured demos folder updates the catalogue via OS filesystem notifications, without periodic directory scans. MCP execution itself does **not** permanently add a demo: save the source in `demos/` and commit it to include it in repository/browser builds.

## Execution and response boundaries

An owned runtime is launched with both `--rustel` and `--score`. Score execution writes a private sibling `.js` file beside the current score to preserve relative asset paths, validates it with an eight-second deadline, then restarts the owned process from that copy. Validation failures keep the previous score playing. A later startup/device/sample failure is still possible; inspect messages and fresh state. Temporary editor files are removed on normal shutdown; originals are not rewritten. The score directory must allow creation of these private copies.

Successful mutations return a correlated request ID, `acknowledged: true`, a message, and a boundary:

```json
{
  "request": "music-mcp-1234-0",
  "acknowledged": true,
  "message": "",
  "boundary": "runtime_control_acknowledgement"
}
```

Transport, tempo and lane controls use `runtime_control_acknowledgement`. Demo loads and source execution use `runtime_score_snapshot`: a nonempty new score snapshot arrived. Neither boundary proves that samples finished downloading, audio reached the speakers, or a GPU frame appeared. Read `get_music_state`, `get_music_score` and `get_music_messages` afterward when verifying an outcome.

Music commands have a ten-second acknowledgement wait and share a 64-pending-command limit with UI controls. If the wait expires, the tool reports **outcome unknown**. A timeout is not cancellation or rollback; do not blindly retry a restart or score execution. Query current source/generation and messages first. Playback restarts and score/demo changes can change generation. Never reuse a cached generation across such changes.

Protocol errors (such as unknown tools or non-object arguments) use the JSON-RPC `error` envelope. Music execution/validation failures use an MCP tool result with `isError: true`; its text content and `structuredContent` contain the error. Notifications without a request ID do not execute mutations.

## Backend support

| Capability | Owned native runtime | External hub producer | Browser/Studio |
| --- | --- | --- | --- |
| Native HTTP MCP endpoint | Yes | Yes, in the desktop viewer | No |
| Read music state/source | Yes, once available | Yes, from producer snapshots | Local UI only |
| Play/Stop | Restart/Stop | Producer behavior | Local UI only |
| Pause/Seek | Unsupported | Requires advertised capability | Local UI only |
| Tempo/lane controls | Existing native control interface | Producer must handle commands | Local UI only |
| Select demo | Native catalogue | Producer must support it | Local catalogue UI |
| Execute arbitrary source via MCP | Yes | Rejected | No native endpoint |

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| Connection refused | Start the desktop app, check `--agent-port`/`--no-agent`, and inspect bind/startup notices. |
| `no such tool` for a music tool | Wait for stage initialization, inspect `tools/list`, and confirm the executable includes music MCP support. |
| No unique live session | Read `get_music_state`; choose a returned session ID, or check the producer/runtime if the list is empty. |
| Stale session/generation | Read fresh state and build a new request using its exact values. |
| Pause/seek rejected | The hosted native runtime exposes Restart/Stop. Only use pause/seek when its producer advertises them. |
| Outcome unknown / acknowledgement timeout | Read source, generation, transport and messages before deciding whether another mutation is needed. |
| Score rejected | Read the tool error and runtime messages. The previous score remains active after validation failure. |
| Score installed but samples are silent | Check messages and asset URLs/sample availability. A score snapshot does not confirm successful sample downloads or audible output. |
| Dropdown says Editor | Unsaved source uses that label. Save a matching `.strudel` in the configured demos folder; only CRLF/LF line-ending differences are ignored. |

## Verification and implementation

Run against an owned desktop instance:

```powershell
python scripts/check_mcp.py
python scripts/check_music_mcp.py
python scripts/check_demo_identity.py "Bachata luna de cobre.strudel"
python scripts/check_demo_watch.py
```

`check_mcp.py` reads the scene endpoint. `check_music_mcp.py` changes playback, executes a test score, verifies invalid-edit recovery and restores the initial demo. `check_demo_identity.py` executes the named saved score and leaves it playing. `check_demo_watch.py` creates/renames/removes a temporary demo and verifies that playback generation stays unchanged.

Implementation: [music tools](../src/music_mcp.rs), [command bridge](../src/bridge.rs), [owned runtime and filesystem notifications](../upstream-adapter/src/host.rs), and [viewer endpoint extension](../patches/viewer-music-extension.patch). The scene tools retain the pinned viewer's own contract and schemas.

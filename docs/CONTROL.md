# Driving Forge Studio from outside itself

The studio is not a closed box. While it is running it exposes a **loopback HTTP
control plane**, and the repository ships an **MCP server** on top of it, so an
agent or a script can do everything the window can do: read and write prompt
sets, dispatch a render, follow a run, list the renders, download them, and use
the OpenRouter ideation calls.

## 1. The control plane

Started automatically with the app. Loopback only, token-gated.

| | |
|---|---|
| Address | `http://127.0.0.1:7317` (override with `FORGE_CONTROL_PORT`) |
| Auth | `Authorization: Bearer <token>` (or `X-Forge-Token`) |
| Discovery | `%APPDATA%\forge-studio\control.json` holds `{port, token, pid, url}` |
| Turn it off | set `FORGE_CONTROL=off` |
| Pin the token | set `FORGE_CONTROL_TOKEN` |

The descriptor file is written so a client never has to guess a port or scrape a
log. It lives beside the token file in your user config directory, readable only
by you.

```bash
# read the endpoint the app published
cat "$APPDATA/forge-studio/control.json"

TOKEN=$(python -c "import json,os;print(json.load(open(os.path.expandvars(r'%APPDATA%/forge-studio/control.json')))['token'])")
curl -s -H "Authorization: Bearer $TOKEN" http://127.0.0.1:7317/status
```

### Endpoints

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/` | | the endpoint list |
| GET | `/status` | | signed-in GitHub user, OpenRouter key state |
| GET | `/sets` | | prompt sets in the pipeline repo |
| GET | `/set/<slug>` | | one set |
| PUT | `/set/<slug>` | prompt set JSON | `{saved: <commit sha>}` |
| POST | `/dispatch` | `{set,only,variants,steps,shards,adhoc}` | `{dispatched: true}` |
| GET | `/runs` | | recent runs |
| GET | `/run/<id>` | | one run with its jobs |
| GET | `/run/<id>/outputs` | | exactly the images that run published |
| GET | `/renders` | | everything on the `renders` branch |
| POST | `/download` | `{urls:[...], dir:"C:/path"}` | `{saved:[...], failed:[...]}` |
| POST | `/ideate` | `{brief, model, count}` | `{set: "<json>"}` |
| POST | `/advise` | `{question, context, model}` | `{answer}` |

Errors come back as `{"error": "..."}` with a non-200 status. Nothing returns a
bare 500 with an empty body.

## 2. The MCP server

`mcp/forge-studio-mcp.mjs` speaks MCP over stdio and forwards every call to the
control plane. It reads the descriptor file itself, so it can only reach a
studio running as the same user on the same machine. If the app is not running,
every tool returns a sentence saying so instead of hanging.

Register it as a stdio MCP server with your agent:

```jsonc
// Hermes: config.yaml
mcp:
  servers:
    forge-studio:
      command: node
      args: ["D:/bots/forge-studio/mcp/forge-studio-mcp.mjs"]
```

Tools exposed: `studio_status`, `list_sets`, `get_set`, `save_set`,
`dispatch_render`, `list_runs`, `get_run`, `run_outputs`, `list_renders`,
`download_images`, `ideate`, `advise`.

## 3. Why loopback and a token

The control plane can start paid work (ideation) and write to your repository, so
it is not left open. It binds `127.0.0.1`, so nothing outside the machine can
reach it, and every request must carry the token from the descriptor file. The
app never prints the token to a log or to the UI.

## 4. Verifying it without a GUI

```bash
# is the studio up and which account?
curl -s -H "Authorization: Bearer $TOKEN" http://127.0.0.1:7317/status

# fire a one-off render that is never committed
SET=$(python -c "import json,base64;print(base64.b64encode(json.dumps({'name':'adhoc','size':[768,1024],'style':'oil painting','items':{'a':{'seed':1,'line':'a lighthouse in a storm'}}}).encode()).decode())")
curl -s -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d "{\"set\":\"all\",\"variants\":\"1\",\"steps\":\"4\",\"shards\":\"1\",\"adhoc\":\"$SET\"}" \
  http://127.0.0.1:7317/dispatch
```

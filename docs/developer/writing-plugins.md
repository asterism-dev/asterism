# Writing plugins

Everything Asterism knows about forges, agents and issue trackers comes from plugins, including the built-in Claude Code, GitHub and Linear support. A plugin is a directory with a `plugin.toml` manifest and, usually, a backend program the daemon talks to over stdin/stdout. Backends can be written in any language.

## Plugin kinds

A plugin declares what it provides under `[provides]`. One plugin may provide several kinds.

| Kind | What it adds | Backend needed |
|---|---|---|
| `forge` | A code host: repository listing, cloning, creating remotes, pull request status and search. | Yes |
| `agent` | A coding agent that can run in a session. | Only for `launch = "backend"` |
| `command` | A subcommand of the `asterism` CLI. | Yes |
| `task_source` | Issues that tasks can be created from. | Yes |
| `panel` | A web view in the desktop app's task dock. | No |

Each forge id, agent id, command name and task source id can only be provided by one plugin. If two plugins claim the same one, the plugin found later is marked broken. Discovery order is: linked plugins, then installed plugins, then the built-in ones; the first plugin with a given name wins.

## Manifest reference

`plugin.toml` is parsed strictly: unknown keys and sections are errors. Ids, names and the plugin name are slugs: lowercase `a-z`, digits and `-`, not starting with `-`.

### Top level

| Field | Type | Required | Meaning |
|---|---|---|---|
| `name` | string (slug) | yes | Plugin name. Must match the name the plugin is linked or installed under. |
| `version` | string | yes | Letters, digits, `.`, `_`, `+` and `-`, not starting with `.`. Installed versions live in a directory of this name. |
| `protocol` | integer | yes | Plugin protocol version. Must be `1`. |
| `description` | string | no | One-line description shown in plugin lists. |
| `permissions` | array of strings | no | What the plugin needs, e.g. `"network"`, `"exec:gh"`, `"ui:sessions"` (a panel may read and focus the task's sessions). Shown to the user, who must accept exactly this list when installing from a store, and again when an update adds permissions. Asterism does not sandbox backends. |
| `backend` | table | see below | How to start the backend. |
| `provides` | table | no | The capabilities, see below. |
| `settings` | array of tables | no | User settings, see [Settings](#settings). |

### `[backend]`

| Field | Type | Required | Meaning |
|---|---|---|---|
| `command` | array of strings | yes, non-empty | Program and arguments. A program containing `/` is resolved relative to the plugin directory; a bare name is looked up on `PATH`. |

A `[backend]` section is required when the plugin provides a forge, a command, a task source or an agent with `launch = "backend"`.

### `[[provides.forge]]`

| Field | Type | Required | Meaning |
|---|---|---|---|
| `id` | string (slug) | yes | Forge id. |
| `display_name` | string | yes | Name shown in the UI. |
| `hosts` | array of strings | no | Git remote hosts this forge serves, e.g. `"github.com"`. Pull request status is fetched from the forge whose `hosts` contains the host of the project's `origin` remote. |
| `pull_requests` | bool | no, default `false` | The backend answers `forge.pull_requests` and `forge.search_pull_requests`. |
| `reviews` | bool | no, default `false` | The backend answers `forge.review.*`. |

### `[[provides.agent]]`

| Field | Type | Required | Meaning |
|---|---|---|---|
| `id` | string (slug) | yes | Agent id. `shell` and `command` are reserved. |
| `display_name` | string | no, defaults to `id` | Name shown in the UI. |
| `binary` | string | yes | The agent's executable. The agent counts as available only when this is found on `PATH`. |
| `launch` | `"static"` or `"backend"` | yes | Where the command line comes from: the templates below, or the backend's `agent.prepare` reply. |
| `start` | array of strings | for `static` | Template for starting the agent. |
| `prompt` | array of strings | no | Appended to `start` when the session starts with a prompt. |
| `resume` | array of strings | no | Template for resuming after a daemon restart. Without it, a static agent is not resumed. |
| `waiting_patterns` | array of strings | no | Screen text that means the agent waits for the user; see [Architecture](architecture.md#session-lifecycle). |
| `settings` | array of `"args"`, `"mcp"`, `"hooks"` | no | Which agent settings sections the user can edit for this agent: extra arguments, MCP servers, hooks. |
| `reserved_args` | array of strings | no | Arguments the user may not add. Entries ending in `=` match as prefixes. |

Templates are expanded token by token. These tokens are replaced only when they make up a whole array element:

| Token | Replaced by |
|---|---|
| `{binary}` | The `binary` value. |
| `{args...}` | The user's extra arguments, zero or more elements. |
| `{prompt}` | The session prompt. |
| `{agent_ref}` | The agent's own session id, as reported through `asterism hook --agent-ref`. Resuming needs one. |

### `[[provides.command]]`

| Field | Type | Required | Meaning |
|---|---|---|---|
| `name` | string (slug) | yes | Subcommand name. Must not clash with a built-in one (`project`, `task`, `session`, `send`, `read`, `wait`, `attach`, `hook`, `daemon`, `plugin`, `store`, `issue`, `pr`, `help`). |
| `description` | string | no | Shown in `asterism plugin list`. |

### `[[provides.task_source]]`

| Field | Type | Required | Meaning |
|---|---|---|---|
| `id` | string (slug) | yes | Task source id. |
| `display_name` | string | yes | Name shown in the UI. |

### `[[provides.panel]]`

| Field | Type | Required | Meaning |
|---|---|---|---|
| `id` | string (slug) | yes | Panel id, unique within the plugin. |
| `title` | string | yes | Shown in the task dock's + menu and as the tab title. |
| `entry` | string | yes | HTML file, relative to the plugin directory, e.g. `"ui/agents.html"`. |
| `slot` | `"task"` | yes | Where the panel can be opened. Only `task` exists. |

A plugin that only provides panels needs no `[backend]`. See [Panels](#panels) for how a panel talks to the app.

## Settings

Each `[[settings]]` entry declares one value the user configures, in the app under Settings → Plugins or with `asterism plugin config <plugin> [key] [value]`.

| Field | Type | Required | Meaning |
|---|---|---|---|
| `key` | string | yes | Lowercase `a-z`, digits and `_`; unique within the plugin. |
| `title` | string | yes | Label shown to the user. |
| `type` | `"string"`, `"secret"`, `"bool"`, `"number"` or `"enum"` | yes | Value type. |
| `required` | bool | no, default `false` | While a required setting has no value, the plugin is in the `needs setup` state and calls to its backend fail with `needs_setup`. |
| `description` | string | no | Help text. |
| `default` | any | no | Must match the type; an `enum` default must be one of `options`. Secrets cannot have a default. |
| `options` | array of strings | for `enum` | Allowed values. |

Non-secret values are stored in `config.toml`, secrets in `secrets.toml` (mode `0600`); secret values are never sent to clients. The backend receives all values, secrets included, in `initialize` and `settings.changed` (see below).

## The backend protocol

The daemon starts the backend lazily, on the first call that needs it, with the plugin directory as working directory. Messages are JSON-RPC 2.0, one JSON object per line on stdin and stdout. Both sides may have several requests in flight, so match responses by `id`. Lines on stdout that are not JSON are logged and ignored; everything on stderr goes to the daemon log, prefixed with `plugin <name>:`.

The backend gets a fixed environment, not the user's shell environment: the daemon's own environment without `CLAUDE*` and `ANTHROPIC_*` variables, plus

| Variable | Value |
|---|---|
| `PATH` | The daemon's directory (with the `asterism` CLI) followed by the daemon's `PATH`. |
| `ASTERISM_HOME` | The data directory. |
| `ASTERISM_SOCKET` | The daemon socket. |
| `ASTERISM_CLI` | Path of the `asterism` CLI. |
| `ASTERISM_PLUGIN_BIN` | The backend program, as resolved from `backend.command`. |

### Lifecycle

- **Handshake.** The first request is always `initialize`. Its reply must list exactly the capabilities the manifest implies, otherwise the plugin is marked failing until it is reloaded:
    - `forge` if it provides a forge, `pull_requests` if any forge has `pull_requests = true`, `reviews` if any forge has `reviews = true`,
    - `agent` if any agent has `launch = "backend"`,
    - `command` if it provides a command, `task_source` if it provides a task source.
- **Timeouts.** Calls time out after 20 s, except `forge.clone` and `forge.create_remote`, which have no timeout.
- **Idle stop.** A backend with no calls for 60 s is stopped and restarted on the next call. Do not keep state in memory that you cannot rebuild; persist it in `data_dir`.
- **Crashes.** A backend that exits unexpectedly 3 times within a minute is marked failing until the plugin is reloaded.
- **Settings.** When the user changes a setting of a running backend, the daemon sends `settings.changed`. If that call fails, for example because the backend does not implement it, the backend is stopped and starts with the new values on its next call.

### Errors

Errors use the JSON-RPC shape with a `data.kind`:

```json
{"jsonrpc": "2.0", "id": 4, "error": {"code": -32001, "message": "no issue ECH-9", "data": {"kind": "not_found"}}}
```

The kinds `invalid_params` (-32602), `not_found` (-32001), `timeout` (-32007) and `git` (-32008) reach the user as they are. Any other kind is reported as `plugin_error`, prefixed with the plugin name.

### Methods the daemon calls

Parameter and result types are defined in `crates/asterism-plugin/src/protocol.rs` and `crates/asterism-proto/src/types.rs`. Every request carries `project_path` where a project is involved, and `forge` or `source` with the id from the manifest, so one backend can serve several of them.

| Method | Capability | Params | Result |
|---|---|---|---|
| `initialize` | all | `protocol`, `plugin_dir`, `data_dir`, `asterism_version`, `settings` | `{"capabilities": [...]}` |
| `settings.changed` | all | `settings` | anything |
| `forge.status` | `forge` | `{}` | `available`, `authenticated`, `account`, `owners`, `error` |
| `forge.list_repos` | `forge` | `owner` | list of `owner`, `name`, `description`, `private` |
| `forge.resolve_owner` | `forge` | `owner`, `visibility` | `owner` |
| `forge.clone` | `forge` | `owner`, `repo`, `target`, `git_env` | anything |
| `forge.create_remote` | `forge` | `owner`, `name`, `visibility`, `dir`, `git_env` | anything |
| `forge.pull_requests` | `pull_requests` | `forge`, `project_path`, `branches` | list of `branch`, `pr` (`number`, `url`, `title`, `state`, `review`, `checks`) |
| `forge.search_pull_requests` | `pull_requests` | `forge`, `project_path`, `query`, `state` (`open`/`closed`) | list of `number`, `title`, `url`, `author`, `head_branch`, `draft`, `from_fork` |
| `forge.review.get` | `reviews` | `forge`, `project_path`, `number` | `title`, `body`, `author`, `url`, `head_sha`, `base_sha`, `head_ref`, `threads`, `conversation`, `checks`, `commit_checks`, `viewed_files`, `pending_review` |
| `forge.review.comment` | `reviews` | `forge`, `project_path`, `number`, `path`, `line`, `side` (`old`/`new`), `body`, `mode` (`single`/`review`) | `null` |
| `forge.review.reply` | `reviews` | `forge`, `project_path`, `number`, `thread_id`, `body` | `null` |
| `forge.review.resolve` | `reviews` | `forge`, `project_path`, `number`, `thread_id`, `resolved` | `null` |
| `forge.review.set_viewed` | `reviews` | `forge`, `project_path`, `number`, `path`, `viewed` | `null` |
| `forge.review.submit` | `reviews` | `forge`, `project_path`, `number`, `event` (`comment`/`approve`/`request_changes`), `body` | `null` |
| `forge.review.add_comment` | `reviews` | `forge`, `project_path`, `number`, `body` | `null` |
| `forge.checks.log` | `reviews` | `forge`, `project_path`, `number`, `check_id` | `text`, `truncated`, `url` |
| `forge.checks.rerun` | `reviews` | `forge`, `project_path`, `number`, `check_id` | `null` |
| `task_source.check` | `task_source` | `source`, `project_path` | `available`, optional `reason` |
| `task_source.search` | `task_source` | `source`, `query`, `assigned_to_me`, `project_path` | list of `key`, `title`, `url`, `state`, `assignee`, `updated_at` |
| `task_source.get` | `task_source` | `source`, `key`, `project_path` | `key`, `title`, `url`, `description`, `branch` |
| `agent.prepare` | `agent` | `agent`, `mode` (`start`/`resume`), `prompt`, `agent_ref`, `settings` (`args`, `mcp_config`, `hooks`), `cwd` | `argv` (non-empty), optional `env` as `[name, value]` pairs |

`visibility` is `public`, `private` or `internal`. In `forge.pull_requests`, `state` is `open`, `draft`, `merged` or `closed`; `review` is `approved`, `changes_requested`, `review_required` or `none`; `checks` is `{"state": ..., "failing": [...]}` with state `pending`, `success`, `failure` or `none`. Leave branches without a pull request out of the result. `task_source.get` may return a `branch` to use for the task; when it is `null`, Asterism derives one.

In `forge.review.get`, a check (in `checks` and `commit_checks`) has `id`, `name`, `workflow`, `status` (`queued`/`running`/`done`), `conclusion`, `started_at`, `completed_at`, `url`, `has_log` and `rerunnable`; a `conversation` item has `id`, `kind` (`comment`/`review`), `author`, `body`, `created_at` and `state`. `forge.checks.*` are only called for checks with `has_log` or `rerunnable` set.

### Calling back into the daemon

A backend may send requests of its own, with method `host.<method>`, where `<method>` is any daemon method a client could call, for example `host.project.list`. The daemon answers on stdin like any JSON-RPC peer. Excluded are `plugin.*`, `store.*`, `task_source.*`, `pr.*`, `shutdown`, `subscribe`, `session.attach` and `session.detach`.

### Command mode

Plugin commands do not use the protocol. `asterism <name> [args...]` replaces itself with the backend command plus `command <name> [args...]`, so the backend's output and exit code become the command's. It runs in the user's terminal environment, with `ASTERISM_SOCKET`, `ASTERISM_CLI` and `ASTERISM_PLUGIN_BIN` added. A backend that provides commands must therefore check its arguments for `command` before starting the protocol loop.

### Rust backends

Backends in Rust can use the `asterism-plugin` crate: `asterism_plugin::serve(handler)` runs the stdin/stdout loop, passes each request to `handler(host, method, params)`, and `Host::call` sends `host.*` requests. The built-in backends in `crates/asterism/src/bin/asterism-plugin-*` are complete examples.

## Panels

A panel is a web page the desktop app shows in a sandboxed iframe (`sandbox="allow-scripts"`) in the task dock. Its files are served from `asterism-plugin://localhost/<plugin>/<path>`, so load scripts, styles and images with paths relative to the entry. The page has no network access (`connect-src 'none'`); everything it knows comes from the app over `postMessage`. The built-in `agents` plugin in `crates/asterism/plugins/agents` is a complete example.

### The bridge

The panel sends calls to `parent` and matches replies by `id`:

```js
parent.postMessage({ id: 1, method: 'sessions.list', params: {} }, '*');
// reply: { id: 1, result: ... }  or  { id: 1, error: { code, message } }
```

| Method | Permission | Params | Result |
|---|---|---|---|
| `context` | none | none | `taskId`, `theme` (`name`: `light`/`dark`, `vars`: CSS variables), `protocol` (`1`) |
| `sessions.list` | `ui:sessions` | none | The task's sessions, each with its `subagents` |
| `events.subscribe` | `ui:sessions` | none | `null`; the panel receives the task's events from now on |
| `ui.focusSession` | `ui:sessions` | `sessionId` | `null`; shows that session's tab |

Error codes are `method_not_found`, `permission_denied` and `invalid_params`. Anything that is not a call is ignored.

The app also sends events as `{ event, data }`. `theme` (same shape as in `context`) arrives when the panel loads and whenever the theme changes, subscribed or not. After `events.subscribe` the panel gets, for the sessions of its task only, `session.changed`, `session.status_changed`, `session.removed`, `subagent.started` and `subagent.updated`, with the daemon's notification params as `data`.

### Reporting subagents

An agent plugin can make subagents show up in panels by calling the CLI from its hooks. Asterism sets `ASTERISM_SESSION` for every session it starts, and the hook commands never fail, so they are safe to call unconditionally:

```sh
asterism hook subagent-start --id <id> [--parent <id>] [--kind <kind>] [--description <text>] [--alias <id>]
asterism hook subagent-stop --id <id> [--failed]
```

`--id` is any id unique within the session; `--parent` nests a subagent under another one. `--alias` is for an agent that learns a second id for a running subagent: repeat `subagent-start` with the same `--id` and `--alias <other>`, and a later `subagent-stop --id <other>` finishes it. Pass values that may start with `-` as `--description=<text>`. The Claude Code plugin (`crates/asterism/src/bin/asterism-plugin-claude/hook.rs`) maps Claude's `Agent` (formerly `Task`) tool calls to these commands; an asynchronous launch is aliased to Claude's agent id and finished by Claude's `SubagentStop` hook.

## Walkthrough: the echo plugin

The test suite drives the plugin runtime with a small Python plugin in `crates/asterism/tests/fixtures/plugins/echo/`. It provides every capability kind, so it is a compact reference. Some behaviour is test-only: the `FIXTURE_*` environment variables and the `echo.*` methods let tests simulate crashes, hangs and garbage output.

### `plugin.toml`

```toml
name = "echo"
version = "0.1.0"
protocol = 1
description = "Test fixture"
permissions = ["network"]

[backend]
command = ["./backend.py"]

[[provides.forge]]
id = "echo-forge"
display_name = "Echo"
hosts = ["echo.test"]
pull_requests = true
reviews = true

[[provides.command]]
name = "echo-cmd"
description = "Prints its arguments"

[[provides.agent]]
id = "echo-agent"
display_name = "Echo agent"
binary = "sh"
launch = "static"
start = ["{binary}", "-c", "echo started \"$@\"; sleep 30", "echo", "{args...}"]
settings = ["args"]
waiting_patterns = ["(y/n)"]

[[provides.task_source]]
id = "echo-issues"
display_name = "Echo issues"

[[settings]]
key = "token"
title = "Token"
type = "secret"
required = true

[[settings]]
key = "region"
title = "Region"
type = "enum"
options = ["eu", "us"]
default = "eu"
```

- The backend is `./backend.py`, resolved relative to the plugin directory. It must be executable.
- `echo-forge` sets `pull_requests = true` and `reviews = true`, so the backend must answer the pull request and review methods and report `pull_requests` and `reviews` in `initialize`.
- `echo-agent` is a static agent: no backend call is needed to start it. Starting it runs `sh -c 'echo started "$@"; sleep 30' echo <args...>`, and `(y/n)` on screen marks the session as waiting for input. Only the `args` agent setting applies.
- `token` is a required secret, so the plugin needs setup until it is set. `region` is an enum with a default.
- The implied backend capabilities are `forge`, `pull_requests`, `reviews`, `command` and `task_source`; the static agent adds none.

### `backend.py`

```python
#!/usr/bin/env python3
"""Fixture backend: behaviour is chosen by method name and FIXTURE_* environment variables."""
import json
import os
import sys
import threading
import time

out_lock = threading.Lock()
pending = {}
next_id = [1000]
ISSUES = {
    "ECH-1": {"title": "Fix login timeout", "description": "Users get logged out.", "branch": "feature/ech-1-fix-login-timeout"},
    "ECH-2": {"title": "Add dark mode", "description": "", "branch": None},
    "ECH-3": {"title": "Bad branch", "description": "", "branch": "bad..branch"},
    "ECH-4": {"title": "!!!", "description": "", "branch": None},
    "%%": {"title": "%%", "description": "", "branch": None},
}


def send(message):
    with out_lock:
        sys.stdout.write(json.dumps(message) + "\n")
        sys.stdout.flush()


def record(line):
    path = os.environ.get("FIXTURE_LOG")
    if path:
        with open(path, "a") as f:
            f.write(line + "\n")


def error(rid, code, kind, message):
    send({"jsonrpc": "2.0", "id": rid, "error": {"code": code, "message": message, "data": {"kind": kind}}})


def host_call(method, params):
    event = threading.Event()
    with out_lock:
        next_id[0] += 1
        rid = next_id[0]
    pending[rid] = {"event": event}
    send({"jsonrpc": "2.0", "id": rid, "method": "host." + method, "params": params})
    event.wait(10)
    return pending.pop(rid).get("reply", {"error": {"code": -32603, "message": "no host reply", "data": {"kind": "internal"}}})


def handle(request):
    method, params, rid = request["method"], request.get("params") or {}, request["id"]
    mode = os.environ.get("FIXTURE_MODE", "")
    if method == "initialize":
        if mode == "crash-init":
            os._exit(3)
        if mode == "hang-init":
            time.sleep(60)
        record("init " + json.dumps(params.get("settings", {}), sort_keys=True))
        caps = [c for c in os.environ.get("FIXTURE_CAPS", "command,forge,task_source,pull_requests,reviews").split(",") if c]
        result = {"capabilities": caps}
    elif method == "forge.status":
        result = {"available": True, "authenticated": True, "account": "me", "owners": ["acme"], "error": None}
    elif method == "task_source.check":
        record("check " + params["project_path"])
        if mode == "no-repo":
            result = {"available": False, "reason": "project has no echo repository"}
        else:
            result = {"available": True}
    elif method == "task_source.search":
        record("search %s %s" % (params.get("query", ""), params.get("assigned_to_me", False)))
        q = params.get("query", "").lower()
        result = [
            {"key": k, "title": v["title"], "url": "https://echo.test/" + k, "state": "open",
             "assignee": "me" if params.get("assigned_to_me") else None, "updated_at": "2026-10-0%dT00:00:00Z" % (i + 1)}
            for i, (k, v) in enumerate(ISSUES.items()) if q in v["title"].lower()
        ]
    elif method == "task_source.get":
        issue = ISSUES.get(params["key"])
        if issue is None:
            error(rid, -32001, "not_found", "no issue " + params["key"])
            return
        result = {"key": params["key"], "title": issue["title"], "url": "https://echo.test/" + params["key"],
                  "description": issue["description"], "branch": issue["branch"]}
    elif method == "forge.pull_requests":
        record("prs " + ",".join(params["branches"]))
        path = os.environ.get("FIXTURE_PRS")
        data = json.load(open(path)) if path and os.path.exists(path) else {}
        if "error" in data:
            error(rid, -32008, "git", data["error"])
            return
        result = [{"branch": b, "pr": data[b]} for b in params["branches"] if b in data]
    elif method == "forge.search_pull_requests":
        record("search " + params.get("state", "open") + " " + params.get("query", ""))
        result = [{"number": 7, "title": "Add search", "url": "https://echo.test/pr/7", "author": "octo",
                   "head_branch": "feature/search", "draft": False, "from_fork": False}]
    elif method.startswith("forge.review."):
        record("review " + method[len("forge.review."):] + " " + json.dumps(params, sort_keys=True))
        path = os.environ.get("FIXTURE_REVIEW")
        data = json.load(open(path)) if path and os.path.exists(path) else None
        if data is None or "error" in data:
            error(rid, -32008, "plugin_error", (data or {}).get("error", "no review fixture"))
            return
        kind = method[len("forge.review."):]
        if kind == "comment":
            data["threads"].append({"id": "F%d" % (len(data["threads"]) + 1), "path": params["path"],
                                    "line": params["line"], "side": params["side"], "outdated": False,
                                    "resolved": False, "pending": params["mode"] == "review",
                                    "comments": [{"id": "c", "author": "me", "body": params["body"], "created_at": ""}]})
            if params["mode"] == "review":
                data["pending_review"] = {"id": "R1", "comments": sum(t["pending"] for t in data["threads"])}
        elif kind == "resolve":
            for t in data["threads"]:
                if t["id"] == params["thread_id"]:
                    t["resolved"] = params["resolved"]
        elif kind == "set_viewed":
            files = set(data["viewed_files"])
            (files.add if params["viewed"] else files.discard)(params["path"])
            data["viewed_files"] = sorted(files)
        elif kind == "add_comment":
            data["conversation"].append({"id": "I%d" % (len(data["conversation"]) + 1), "kind": "comment",
                                         "author": "me", "body": params["body"], "created_at": "", "state": None})
        elif kind == "submit":
            data["pending_review"] = None
            for t in data["threads"]:
                t["pending"] = False
        if kind != "get":
            json.dump(data, open(path, "w"))
        result = data if kind == "get" else None
    elif method.startswith("forge.checks."):
        kind = method[len("forge.checks."):]
        record("checks " + kind + " " + json.dumps(params, sort_keys=True))
        path = os.environ.get("FIXTURE_REVIEW")
        data = json.load(open(path)) if path and os.path.exists(path) else {}
        check = next((c for c in data.get("checks", []) if c["id"] == params["check_id"]), None)
        if check is None or (kind == "log" and params["check_id"] not in data.get("logs", {})):
            error(rid, -32008, "plugin_error", "no log for " + params["check_id"])
            return
        if kind == "log":
            result = {"text": data["logs"][params["check_id"]], "truncated": False, "url": check["url"]}
        else:
            check["status"], check["conclusion"] = "queued", None
            json.dump(data, open(path, "w"))
            result = None
    elif method == "echo.sleep":
        time.sleep(params.get("ms", 0) / 1000)
        result = params
    elif method == "echo.crash":
        os._exit(3)
    elif method == "echo.garbage":
        with out_lock:
            sys.stdout.write("this is not json\n")
            sys.stdout.flush()
        result = "after garbage"
    elif method == "echo.host":
        reply = host_call(params["method"], params.get("params"))
        if "error" in reply:
            send({"jsonrpc": "2.0", "id": rid, "error": reply["error"]})
            return
        result = reply.get("result")
    elif method == "settings.changed" and mode != "no-settings":
        record("settings " + json.dumps(params["settings"], sort_keys=True))
        result = None
    else:
        error(rid, -32601, "method_not_found", "unknown method " + method)
        return
    send({"jsonrpc": "2.0", "id": rid, "result": result})


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "command":
        args = sys.argv[2:]
        print(" ".join(args))
        sys.stdout.flush()
        sys.exit(int(args[args.index("--exit") + 1]) if "--exit" in args else 0)
    record("start %d" % os.getpid())
    for line in sys.stdin:
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if "method" in message and "id" in message:
            threading.Thread(target=handle, args=(message,), daemon=True).start()
        elif "id" in message and message["id"] in pending:
            pending[message["id"]]["reply"] = message
            pending[message["id"]]["event"].set()


main()
```

How it maps to the protocol:

- `main` first checks for command mode: `backend.py command echo-cmd hello` prints `echo-cmd hello` and exits, with the code given after `--exit`. Otherwise it reads stdin line by line.
- Each request (a message with `method` and `id`) is handled in its own thread, so slow calls do not block others. Replies to the backend's own `host.*` requests (messages with an `id` it is waiting for) wake the waiting thread.
- `send` serialises writes with a lock and flushes after every line. Without the flush, the daemon would not see the reply.
- `initialize` returns the capabilities; the default list matches what the manifest implies.
- `task_source.search`, `task_source.get`, `forge.status`, `forge.pull_requests` and `forge.search_pull_requests` return the shapes from the [method table](#methods-the-daemon-calls). `task_source.get` shows a `not_found` error.
- Unknown methods return `method_not_found`. With `FIXTURE_MODE=no-settings` this includes `settings.changed`, which makes the daemon restart the backend with the new settings instead.
- `echo.host` forwards a request to the daemon as `host.<method>` and relays the reply, showing the callback direction.

### Trying it

Link the directory into your daemon, ideally a development one (see [Building from source](building.md#development-instance)):

```sh
asterism plugin link crates/asterism/tests/fixtures/plugins/echo
asterism plugin list
asterism plugin config echo token     # prompts for the secret
asterism echo-cmd hello --exit 0
```

`plugin link` records the directory in `plugins/links.toml` and reloads all plugins. A linked plugin is used in place, so edits to the backend take effect when it next starts; after editing `plugin.toml`, run `asterism plugin reload`. `asterism plugin unlink echo` removes the link. The daemon log (`asterismd.log` in `ASTERISM_HOME`) shows the backend's stderr and why a plugin is broken or failing.

## Publishing through a store

A store is a git repository, or a local directory, with a `store.json` at its root:

```json
{
  "format": 1,
  "name": "my-store",
  "description": "Plugins for my team",
  "plugins": [
    {"name": "echo", "description": "Example plugin", "tags": ["example"], "path": "plugins/echo"},
    {"name": "other", "git": "https://example.com/other-plugin.git", "ref": "v1.0.0"}
  ]
}
```

| Field | Meaning |
|---|---|
| `format` | Must be `1`. |
| `name` | Store name, a slug. |
| `description` | Optional. |
| `plugins[].name` | Plugin name, a slug, unique in the store; must equal the `name` in the plugin's `plugin.toml`. |
| `plugins[].description`, `plugins[].tags` | Optional, used by `asterism plugin search`. |
| `plugins[].path` | Plugin directory, relative and inside the repository; defaults to the root for git entries. |
| `plugins[].git`, `plugins[].ref` | A plugin in a separate git repository at a fixed ref. Both or neither. |

Each entry needs either a `path` or a `git` source. Users add a store with `asterism store add <git-url-or-absolute-path>` and install with `asterism plugin install <name>`. An install copies the plugin directory to `plugins/installed/<name>/<version>/`; the previous version is kept for `asterism plugin rollback`. The official store, `https://github.com/asterism-dev/asterism-plugins.git`, is configured by default.

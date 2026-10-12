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

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
        if mode == "hang-init":
            time.sleep(60)
        record("init " + json.dumps(params.get("settings", {}), sort_keys=True))
        caps = [c for c in os.environ.get("FIXTURE_CAPS", "command,forge").split(",") if c]
        result = {"capabilities": caps}
    elif method == "forge.status":
        result = {"available": True, "authenticated": True, "account": "me", "owners": ["acme"], "error": None}
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

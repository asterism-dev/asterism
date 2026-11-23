#!/usr/bin/env python3
"""Fixture machine provider: VMs live in the JSON file $FIXTURE_VMS as {"vms": {...}, "calls": [...]}."""
import json
import os
import sys
import threading

lock = threading.Lock()
CREATE_OPTIONS = [
    {"key": "cpus", "title": "CPUs", "type": "number", "default": 2},
    {"key": "image", "title": "Image", "type": "enum", "options": ["base", "gpu"], "default": "base"},
]


def send(message):
    with lock:
        sys.stdout.write(json.dumps(message) + "\n")
        sys.stdout.flush()


def load():
    path = os.environ["FIXTURE_VMS"]
    try:
        with open(path) as f:
            return json.load(f)
    except (OSError, ValueError):
        return {"vms": {}, "calls": []}


def save(state):
    with open(os.environ["FIXTURE_VMS"], "w") as f:
        json.dump(state, f)


def vm_view(ref, vm):
    return {"ref": ref, "name": vm["name"], "state": vm["state"], "destination": vm["destination"]}


def call(method, params):
    state = load()
    state["calls"].append({"method": method, "params": params})
    vms = state["vms"]
    ref = params.get("ref")
    if os.environ.get("FIXTURE_FAIL") == method:
        save(state)
        raise RuntimeError("forced failure in " + method)
    if method == "machine_provider.list":
        result = {"vms": [vm_view(r, v) for r, v in vms.items()], "create_options": CREATE_OPTIONS}
    elif method == "machine_provider.create":
        ref = "vm-%d" % (len(vms) + 1)
        vms[ref] = {"name": params["name"], "state": "running", "destination": os.environ.get("FIXTURE_DESTINATION", "")}
        result = {"ref": ref, "name": params["name"], "destination": vms[ref]["destination"]}
    elif method not in ("machine_provider.resume", "machine_provider.hibernate", "machine_provider.delete"):
        save(state)
        raise LookupError(method)
    elif ref not in vms:
        save(state)
        raise RuntimeError("unknown vm " + str(ref))
    elif method == "machine_provider.resume":
        vms[ref]["state"] = "running"
        vms[ref]["destination"] = os.environ.get("FIXTURE_RESUME_DESTINATION") or vms[ref]["destination"]
        result = {"destination": vms[ref]["destination"]}
    elif method == "machine_provider.hibernate":
        vms[ref]["state"] = "hibernated"
        result = None
    else:
        del vms[ref]
        result = None
    save(state)
    return result


def handle(request):
    method, params, rid = request["method"], request.get("params") or {}, request["id"]
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": rid, "result": {"capabilities": ["machine_provider"]}})
        return
    if method == "settings.changed":
        send({"jsonrpc": "2.0", "id": rid, "result": None})
        return
    try:
        with state_lock:
            result = call(method, params)
    except LookupError as e:
        send({"jsonrpc": "2.0", "id": rid, "error": {"code": -32601, "message": str(e), "data": {"kind": "method_not_found"}}})
    except Exception as e:
        send({"jsonrpc": "2.0", "id": rid, "error": {"code": -32008, "message": str(e), "data": {"kind": "plugin_error"}}})
    else:
        send({"jsonrpc": "2.0", "id": rid, "result": result})


state_lock = threading.Lock()

for line in sys.stdin:
    try:
        message = json.loads(line)
    except ValueError:
        continue
    if "method" in message and "id" in message:
        threading.Thread(target=handle, args=(message,), daemon=True).start()

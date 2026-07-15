#!/usr/bin/env python3
"""Probe for tethys-2mjj: when does rust-analyzer become answerable?

Speaks raw JSON-RPC to rust-analyzer over stdio against the REAL tethys
workspace (cold or warm — pass nothing, it uses cwd). Two independent
readiness measurements run simultaneously:

  A (signal channel): every $/progress begin/report/end notification and
    every experimental/serverStatus notification, timestamped.
  B (oracle channel): poll textDocument/definition at the call site of
    `from_initialize_result` in src/lsp/transport.rs every 500ms from t=0,
    recording each outcome (error code / null / empty / HIT) timestamped.

Agreement to check: the first HIT in B should coincide with (or follow
within one poll interval of) a terminal signal in A — and polls before
that signal should be null/empty/error. Disagreement in either direction
kills the "gate Pass 3 on the progress signal" design.
"""
import json, os, subprocess, sys, threading, time

WS = os.getcwd()
TARGET_FILE = "src/lsp/transport.rs"
NEEDLE = "PositionEncoding::from_initialize_result"
T0 = time.monotonic()

def ts():
    return f"{time.monotonic() - T0:7.2f}s"

lines = open(TARGET_FILE, encoding="utf-8").read().splitlines()
line_idx, col = next((i, l.index(NEEDLE) + len("PositionEncoding::"))
                     for i, l in enumerate(lines) if NEEDLE in l)
print(f"target: {TARGET_FILE}:{line_idx + 1} col(byte,0-idx)={col}")

proc = subprocess.Popen(["rust-analyzer"], stdin=subprocess.PIPE,
                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
seq, pending, lock = 0, {}, threading.Lock()

def send(method, params, is_req=True):
    global seq
    msg = {"jsonrpc": "2.0", "method": method, "params": params}
    if is_req:
        seq += 1
        msg["id"] = seq
        with lock:
            pending[seq] = {"method": method, "t": time.monotonic()}
    raw = json.dumps(msg).encode()
    proc.stdin.write(f"Content-Length: {len(raw)}\r\n\r\n".encode() + raw)
    proc.stdin.flush()
    return seq if is_req else None

results = {}  # request id -> response message

def reader():
    f = proc.stdout
    while True:
        clen = None
        while True:
            line = f.readline()
            if not line:
                return
            if line.lower().startswith(b"content-length:"):
                clen = int(line.split(b":")[1])
            if line == b"\r\n":
                break
        m = json.loads(f.read(clen))
        meth = m.get("method")
        if meth == "$/progress":
            v = m["params"]["value"]
            print(f"{ts()} PROGRESS {m['params']['token']} {v.get('kind')}"
                  f" title={v.get('title', '')!r} msg={v.get('message', '')!r}")
        elif meth == "experimental/serverStatus":
            print(f"{ts()} SERVERSTATUS {json.dumps(m['params'])}")
        elif meth and "id" in m:  # server->client request: ack with null
            resp = json.dumps({"jsonrpc": "2.0", "id": m["id"], "result": None}).encode()
            proc.stdin.write(f"Content-Length: {len(resp)}\r\n\r\n".encode() + resp)
            proc.stdin.flush()
        elif "id" in m:
            results[m["id"]] = m

threading.Thread(target=reader, daemon=True).start()

uri = f"file://{WS}/{TARGET_FILE}"
send("initialize", {
    "processId": os.getpid(), "rootUri": f"file://{WS}",
    "capabilities": {
        "window": {"workDoneProgress": True},
        "general": {"positionEncodings": ["utf-8", "utf-16"]},
        "experimental": {"serverStatusNotification": True},
    }})
while 1 not in results:
    time.sleep(0.05)
enc = results[1]["result"]["capabilities"].get("positionEncoding")
print(f"{ts()} initialized; negotiated encoding={enc}")
send("initialized", {}, is_req=False)
send("textDocument/didOpen", {"textDocument": {
    "uri": uri, "languageId": "rust", "version": 1,
    "text": open(TARGET_FILE, encoding="utf-8").read()}}, is_req=False)

deadline = time.monotonic() + 600
hits = 0
while time.monotonic() < deadline and hits < 3:
    rid = send("textDocument/definition", {
        "textDocument": {"uri": uri},
        "position": {"line": line_idx, "character": col}})
    t_sent = time.monotonic()
    while rid not in results and time.monotonic() - t_sent < 65:
        time.sleep(0.05)
    m = results.pop(rid, None)
    if m is None:
        print(f"{ts()} POLL id={rid} TIMED OUT (>65s)")
    elif "error" in m:
        e = m["error"]
        print(f"{ts()} POLL id={rid} ERROR code={e.get('code')} msg={e.get('message')!r}")
    else:
        r = m.get("result")
        if not r:
            print(f"{ts()} POLL id={rid} EMPTY ({r!r})")
        else:
            loc = r[0] if isinstance(r, list) else r
            u = loc.get("uri") or loc.get("targetUri")
            rng = loc.get("range") or loc.get("targetSelectionRange")
            print(f"{ts()} POLL id={rid} HIT {u.rsplit('/', 1)[-1]}"
                  f":{rng['start']['line'] + 1}")
            hits += 1
    time.sleep(0.5)

proc.kill()
print(f"{ts()} done ({hits} hits)")

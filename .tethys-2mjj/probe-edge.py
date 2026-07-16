#!/usr/bin/env python3
"""Edge probe: what does serverStatus report when there is NO Cargo project?

Informs the fallback posture: if quiescent never flips true (or health is
error), the wait must time out and proceed rather than hang Pass 3.
Usage: probe-edge.py <workspace-dir>
"""
import json, os, subprocess, sys, threading, time

WS = os.path.abspath(sys.argv[1])
T0 = time.monotonic()
proc = subprocess.Popen(["rust-analyzer"], stdin=subprocess.PIPE,
                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)

def send(msg):
    raw = json.dumps(msg).encode()
    proc.stdin.write(f"Content-Length: {len(raw)}\r\n\r\n".encode() + raw)
    proc.stdin.flush()

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
        t = f"{time.monotonic() - T0:6.2f}s"
        meth = m.get("method")
        if meth == "experimental/serverStatus":
            print(f"{t} SERVERSTATUS {json.dumps(m['params'])}")
        elif meth == "$/progress":
            v = m["params"]["value"]
            print(f"{t} PROGRESS {m['params']['token']} {v.get('kind')} {v.get('title', '')!r}")
        elif meth and "id" in m:
            send({"jsonrpc": "2.0", "id": m["id"], "result": None})
        elif "id" in m and m["id"] == 1:
            print(f"{t} initialize responded")
            send({"jsonrpc": "2.0", "method": "initialized", "params": {}})

threading.Thread(target=reader, daemon=True).start()
send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
    "processId": os.getpid(), "rootUri": f"file://{WS}",
    "capabilities": {"window": {"workDoneProgress": True},
                     "experimental": {"serverStatusNotification": True}}}})
time.sleep(12)
proc.kill()
print("done")

# /// script
# dependencies = ["pywinrm==0.5.0"]
# ///
"""Run a PowerShell script file on the local Windows test VM over WinRM. Usage: vmps.py <script.ps1> [timeout_sec]
Credentials file (KEY=VALUE lines RFS_WINDOWS_USER / RFS_WINDOWS_PASSWORD): $PROBE_VM_CREDENTIALS or ~/.config/tethys-probe/windows-vm/credentials.env."""
import os
import sys
from pathlib import Path
import winrm

vals = dict(
    line.split("=", 1)
    for line in Path(os.environ.get("PROBE_VM_CREDENTIALS", Path.home() / ".config/tethys-probe/windows-vm/credentials.env")).read_text().splitlines()
    if "=" in line
)
timeout = int(sys.argv[2]) if len(sys.argv) > 2 else 600
session = winrm.Session(
    "http://127.0.0.1:55985/wsman",
    auth=(vals["RFS_WINDOWS_USER"], vals["RFS_WINDOWS_PASSWORD"]),
    transport="basic",
    server_cert_validation="ignore",
    read_timeout_sec=timeout + 30,
    operation_timeout_sec=timeout,
)
script = Path(sys.argv[1]).read_text()
r = session.run_ps(script)
print("status", r.status_code)
print(r.std_out.decode(errors="replace"))
err = r.std_err.decode(errors="replace")
if err and not err.startswith("#< CLIXML"):
    print("STDERR:", err[:2000])

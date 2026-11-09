"""Run on a native desktop runner (Linux: under dbus-run-session + Xvfb).

This verifies JS initialization, IPC and liveness, not appearance or GPU throughput.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parents[1]
version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
os_name, arch = os.environ['EXPECTED_PLATFORM'], os.environ['EXPECTED_ARCH']
folder = ROOT / f'dist/HashHelm-{version}-{os_name}-{arch}'
exe = folder / ('HashHelm.app/Contents/MacOS/HashHelm' if os_name == 'macos' else 'HashHelm')
info = json.loads(subprocess.run([str(exe), '--hashhelm-platform-info'], check=True, capture_output=True, text=True, timeout=15).stdout)
assert info['platform'] == os_name and {'x86_64':'x64','aarch64':'arm64'}[info['arch']] == arch
with tempfile.TemporaryDirectory() as temp:
    ready = Path(temp) / 'ready.json'
    env = {**os.environ, 'HASHHELM_DATA_DIR': temp, 'HASHHELM_STARTUP_REPORT': str(ready)}
    with (Path(temp) / 'stderr.txt').open('w+') as err:
        child = subprocess.Popen([str(exe)], env=env, stdout=subprocess.DEVNULL, stderr=err)
        try:
            deadline=time.monotonic()+25
            initialized=None
            while child.poll() is None and time.monotonic()<deadline:
                try: initialized=json.loads(ready.read_text());break
                except (FileNotFoundError,json.JSONDecodeError):time.sleep(0.1)
            assert initialized, 'Native frontend did not complete its IPC initialization'
            assert initialized['uiInitialized'] and initialized['platform']==os_name
            assert child.poll() is None, 'Native GUI exited during startup'
        finally:
            if child.poll() is None: child.terminate()
            try: child.wait(timeout=8)
            except subprocess.TimeoutExpired: child.kill(); child.wait(timeout=5)
        err.seek(0)
        stderr = err.read()
# Exercise the packaged engine through the same process layer as the GUI.
# Help, cancellation and result queries must leave the app seal intact too.
engine = Path(info['engine'])
subprocess.run([str(ROOT / 'target/release/runtime-check'), str(engine)], check=True, timeout=180)
if os_name == 'macos':
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(folder / 'HashHelm.app')], check=True)
report = {'platform':os_name, 'arch':arch, 'platformInfo':info, 'uiHandshake':initialized, 'stderr':stderr, 'packagedRuntimeCheck':True, 'signatureVerifiedAfterEngineUse':os_name == 'macos', 'limitations':['JS/IPC and engine process checks; no visual or GPU-performance acceptance.']}
output = ROOT / 'tests/artifacts/native-startup.json'
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(report, indent=2) + '\n')
print(f'Native startup passed: {os_name} {arch}')

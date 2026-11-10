"""Run on a native desktop runner (Linux: under dbus-run-session + Xvfb).

This verifies JS initialization, IPC and liveness, not appearance or GPU throughput.
"""
import json
import os
from pathlib import Path
import subprocess
import shutil
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parents[1]
version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
os_name, arch = os.environ['EXPECTED_PLATFORM'], os.environ['EXPECTED_ARCH']
folder = ROOT / f'dist/HashHelm-{version}-{os_name}-{arch}'
exe = Path(os.environ['HASHHELM_GUI_PATH']) if os.environ.get('HASHHELM_GUI_PATH') else folder / ('HashHelm.app/Contents/MacOS/HashHelm' if os_name == 'macos' else 'HashHelm.exe' if os_name == 'windows' else 'HashHelm')
info = json.loads(subprocess.run([str(exe), '--hashhelm-platform-info'], check=True, capture_output=True, text=True, timeout=15).stdout)
assert info['platform'] == os_name and {'x86_64':'x64','aarch64':'arm64'}[info['arch']] == arch
scratch = ROOT / 'tests/artifacts'
scratch.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix='native-startup-',dir=scratch) as temp:
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
            runtime_processes=[]
            if os_name=='windows':
                command=f"Get-CimInstance Win32_Process -Filter 'ParentProcessId = {child.pid}' | Select-Object ExecutablePath,CommandLine | ConvertTo-Json -Compress"
                raw=subprocess.check_output(['powershell','-NoProfile','-Command',command],text=True,encoding='utf8',errors='replace',timeout=15,creationflags=subprocess.CREATE_NO_WINDOW)
                runtime_processes=json.loads(raw) if raw.strip() else []
                if isinstance(runtime_processes,dict):runtime_processes=[runtime_processes]
                browsers=[p for p in runtime_processes if 'msedgewebview2.exe' in (p.get('ExecutablePath') or '').lower()]
                assert browsers,'No WebView2 child process observed'
                for browser in browsers:
                    args=browser.get('CommandLine') or ''
                    assert 'msSmartScreenProtection' not in args and '--no-sandbox' not in args and '--disable-web-security' not in args,args
                bundled=exe.parent/'runtime/webview2'
                if bundled.is_dir():assert all(Path(p['ExecutablePath']).is_relative_to(bundled) for p in browsers)
            application_events=[]
            for log in (Path(temp)/'logs').glob('app-*.jsonl'):
                application_events.extend(json.loads(line) for line in log.read_text(encoding='utf8').splitlines())
            assert any(e.get('event')=='frontend_ready' for e in application_events),'Frontend readiness was not logged'
        finally:
            if child.poll() is None: child.terminate()
            try: child.wait(timeout=8)
            except subprocess.TimeoutExpired: child.kill(); child.wait(timeout=5)
        err.seek(0)
        stderr = err.read()
    # WebView2 can release its profile locks shortly after the GUI has exited.
    # Remove only the profile in this test's freshly allocated temporary folder.
    for name in ('webview','webview-secure'):
        profile=Path(temp).resolve()/name
        assert profile.parent==Path(temp).resolve() and not profile.is_symlink()
        for attempt in range(50):
            try:
                if profile.exists():shutil.rmtree(profile)
                break
            except FileNotFoundError:
                # WebView2 may remove its own lockfile while rmtree enumerates it.
                # Retry the remaining tree, or finish if the whole profile is gone.
                if not profile.exists():break
            except OSError as error:
                if getattr(error,'winerror',None) not in (5,32,145) or attempt==49:raise
                time.sleep(0.1)
# Exercise the packaged Unix engine without invalidating the macOS app seal.
packaged_runtime_check = os_name != 'windows'
if packaged_runtime_check:
    subprocess.run([str(ROOT / 'target/release/runtime-check'), str(Path(info['engine']))], check=True, timeout=180)
if os_name == 'macos':
    app = next(parent for parent in exe.parents if parent.suffix == '.app')
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], check=True)
    from macos_engine_smoke import check as check_engine_seal
    check_engine_seal(app)
report = {'platform':os_name, 'arch':arch, 'platformInfo':info, 'uiHandshake':initialized,'packagedRuntimeCheck':packaged_runtime_check,'signatureVerifiedAfterEngineUse':os_name=='macos', 'stderr':stderr, 'runtimeProcesses':runtime_processes,'applicationEvents':application_events,'limitations':['JS/IPC startup and runtime security arguments only; no visual or GPU-performance acceptance.']}
report['pidCacheAndForcedTerminationSignatureVerified'] = os_name == 'macos'
output = ROOT / 'tests/artifacts/native-startup.json'
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(report, indent=2) + '\n')
print(f'Native startup passed: {os_name} {arch}')

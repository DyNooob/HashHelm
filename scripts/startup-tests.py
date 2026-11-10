"""Startup diagnostics regression; test-only data, no user settings or evidence."""
import json,os,shutil,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
EXE=Path(os.environ.get('HASHHELM_GUI_PATH',str(ROOT/'target/release'/('hashhelm.exe' if os.name=='nt' else 'hashhelm'))))
OUT=ROOT/'tests/artifacts/startup-regression';OUT.mkdir(parents=True,exist_ok=True)

def check(exe,data,**overrides):
    env={**os.environ,'HASHHELM_DATA_DIR':str(data),**overrides}
    return subprocess.run([str(exe),'--hashhelm-check'],env=env,capture_output=True,text=True,encoding='utf8',errors='replace',timeout=25)

def strings(value):
    if isinstance(value,str):yield value
    elif isinstance(value,dict):
        for item in value.values():yield from strings(item)
    elif isinstance(value,list):
        for item in value:yield from strings(item)

def main():
    results=[]
    with tempfile.TemporaryDirectory(prefix='case-',dir=OUT) as temp:
        temp=Path(temp);data=temp/'data';data.mkdir()
        secret='DO-NOT-EXPORT-RECOVERED-PASSWORD'
        (data/'settings.json').write_text(json.dumps({'secret':secret}))
        (data/'recovered.potfile').write_text(secret)
        (data/'runs').mkdir();(data/'runs/secret.log').write_text(secret)
        good=check(EXE,data);assert good.returncode==0,good.stderr
        support=json.loads(good.stdout)
        assert secret not in good.stdout
        assert not any(str(temp) in s or str(ROOT) in s or str(Path.home()) in s for s in strings(support)),support
        logs=list((data/'logs').glob('app-*.jsonl'));assert logs
        assert any(json.loads(line)['event']=='starting' for line in logs[0].read_text(encoding='utf8').splitlines())
        results.append('headless check, persistent startup log, source-path redaction and sensitive-file exclusion')
        if os.name=='nt':
            missing=check(EXE,temp/'missing-data',WEBVIEW2_BROWSER_EXECUTABLE_FOLDER=str(temp/'missing-runtime'))
            assert missing.returncode!=0 and 'WebView2' in missing.stderr
            assert any('startup_check_failed' in p.read_text(encoding='utf8') for p in (temp/'missing-data/logs').glob('*.jsonl'))
            results.append('missing runtime returns a visible diagnostic instead of silent success')
            # A non-writable portable workspace is modeled by an existing regular
            # file. No ACLs or permissions outside the test folder are changed.
            app=temp/'portable';app.mkdir();(app/'assets').mkdir();(app/'workspace').write_text('preserve original')
            local=temp/'localappdata';shutil.copyfile(EXE,app/'HashHelm.exe')
            env={**os.environ,'LOCALAPPDATA':str(local)};env.pop('HASHHELM_DATA_DIR',None);env.pop('FORENHC_DATA_DIR',None)
            if (EXE.parent/'runtime/webview2/msedgewebview2.exe').is_file():env['WEBVIEW2_BROWSER_EXECUTABLE_FOLDER']=str(EXE.parent/'runtime/webview2')
            run=subprocess.run([str(app/'HashHelm.exe'),'--hashhelm-check'],env=env,capture_output=True,text=True,encoding='utf8',timeout=25)
            assert run.returncode==0,run.stderr
            assert (app/'workspace').read_text()=='preserve original'
            assert any('data_directory_fallback' in p.read_text(encoding='utf8') for p in (local/'DigiForensics/HashHelm/logs').glob('*.jsonl'))
            results.append('portable workspace failure falls back to user data without overwriting original files')
    (OUT/'results.json').write_text(json.dumps({'passed':results},indent=2),encoding='utf8')
    print('\n'.join('PASS '+result for result in results))
if __name__=='__main__':main()

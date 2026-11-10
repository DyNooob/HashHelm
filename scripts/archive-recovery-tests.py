"""Real Hashcat integration for newly supported synthetic/public archives."""
import hashlib,json,os,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
BASE=ROOT/'tests/fixtures/import';OUT=ROOT/'tests/artifacts/archive-recovery';OUT.mkdir(parents=True,exist_ok=True)
engine=Path(os.environ.get('HASHHELM_ENGINE',str(ROOT/'vendor/hashcat-8.0.0/hashcat.exe'))).resolve()
extractor=ROOT/'target'/os.environ.get('HASHHELM_TEST_PROFILE','debug')/('extract-check.exe' if os.name=='nt' else 'extract-check')
dictionary=OUT/'dictionary.txt';dictionary.write_text('wrong\nForenHC2026!\nunrar\n',encoding='utf8')
cache=OUT/'cache';cache.mkdir(exist_ok=True);results=[]
for name in ['zip64.zip','seven.7z','seven-headers.7z','seven-store.7z','seven-lzma.7z','rar5.rar','rar5-headers.rar','rar3.rar','rar3-headers.rar','rar3-store.rar']:
    source=BASE/name;sha=hashlib.sha256(source.read_bytes()).hexdigest()
    info=json.loads(subprocess.run([str(extractor),str(source)],capture_output=True,text=True,encoding='utf8',check=True).stdout)
    target=OUT/(name+'.hash');target.write_text(info['hash']+'\n',encoding='utf8')
    args=[str(engine),'-m',str(info['mode']),'-a','0','--potfile-disable','--restore-disable','--logfile-disable','--cache-path',str(cache),'--runtime','60','--quiet']
    if os.name=='nt':args+=['--backend-ignore-cuda']
    run=subprocess.run(args+[str(target),str(dictionary)],cwd=engine.parent,capture_output=True,text=True,encoding='utf8',errors='replace',timeout=120)
    (OUT/(name+'.log')).write_text(run.stdout+'\n'+run.stderr,encoding='utf8')
    passed=run.returncode==0 and (':ForenHC2026!' in run.stdout or ':unrar' in run.stdout) and sha==hashlib.sha256(source.read_bytes()).hexdigest()
    results.append({'file':name,'mode':info['mode'],'exit':run.returncode,'passed':passed});print(json.dumps(results[-1]),flush=True)
    if not passed:print((run.stdout+run.stderr)[-1600:],flush=True)
(OUT/'results.json').write_text(json.dumps(results,indent=2),encoding='utf8')
if not all(r['passed'] for r in results):raise SystemExit(1)

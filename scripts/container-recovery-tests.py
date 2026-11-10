"""Optional real-Hashcat interoperability checks; requires a working compute backend."""
from pathlib import Path
import json,subprocess,time,os,hashlib
ROOT=Path(__file__).resolve().parents[1];BASE=ROOT/'tests/fixtures/containers';OUT=ROOT/'tests/artifacts/container-recovery';OUT.mkdir(parents=True,exist_ok=True)
ENGINE=Path(os.environ.get('HASHCAT_ENGINE',ROOT/'vendor/hashcat-8.0.0/hashcat.exe')).resolve()
IMPORT=ROOT/('target/release/container-check.exe' if os.name=='nt' else 'target/release/container-check')
VERIFY=ROOT/('target/release/verify-check.exe' if os.name=='nt' else 'target/release/verify-check')
cases=json.loads((BASE/'manifest.json').read_text());results=[]
for name in ['hashcat-29422.hc','hashcat-29423.hc','hashcat-29451.hc','pim-one.hc','keyfile-pim-one.hc']:
    case=next(c for c in cases if c['file']==name);folder=OUT/(str(time.time_ns())+'-'+name);folder.mkdir();mode=case['mode']
    spec={'prf':'sha256' if mode==29451 else 'sha512','bits':mode%10*512,'header':'primary','pim_start':case.get('pim',0),'pim_stop':case.get('pim',0),'keyfiles':[str(BASE/case['keyfile'])] if case.get('keyfile') else []}
    options=folder/'options.json';options.write_text(json.dumps(spec))
    p=subprocess.run([str(IMPORT),str(BASE/name),str(options)],check=True,capture_output=True,text=True,encoding='utf-8');prepared=json.loads(p.stdout)
    target=folder/'target.hash';target.write_text(prepared['extracted']['hash']+'\n');words=folder/'words.txt';words.write_text('wrong-public-password\n'+case['password']+'\n')
    cache=OUT/'cache';cache.mkdir(exist_ok=True);pot=folder/'result.pot';session='vc-qa-'+str(time.time_ns())
    args=['-m',str(mode),'-a','0',str(target),str(words),'--session',session,'--potfile-path',str(pot),'--restore-disable','--cache-path',str(cache),'--logfile-disable','--runtime','30','-w','1','--backend-ignore-cuda']
    if spec['pim_start']:args+=['--veracrypt-pim-start',str(spec['pim_start']),'--veracrypt-pim-stop',str(spec['pim_stop'])]
    if spec['keyfiles']:args+=['--veracrypt-keyfiles',','.join(prepared['record']['container']['spec']['keyfiles'])]
    run=subprocess.run([str(ENGINE),*args],cwd=ENGINE.parent,capture_output=True,text=True,encoding='utf-8',errors='replace',timeout=180)
    (folder/'stdout.txt').write_text(run.stdout,encoding='utf-8');(folder/'stderr.txt').write_text(run.stderr,encoding='utf-8')
    assert run.returncode==0,(name,run.returncode,run.stdout[-3000:],run.stderr[-2000:])
    show=subprocess.run([str(ENGINE),'-m',str(mode),str(target),'--show','--outfile-format','2','--potfile-path',str(pot),'--logfile-disable','--cache-path',str(cache)],cwd=ENGINE.parent,capture_output=True,text=True,encoding='utf-8',errors='replace',timeout=30)
    assert show.returncode==0 and case['password'] in show.stdout,(name,show.stdout,show.stderr)
    recovered=next(line for line in show.stdout.splitlines() if case['password'] in line)
    request={'record':prepared['record'],'password_hex':recovered.encode().hex(),'destination':None}
    verified=subprocess.run([str(VERIFY)],input=json.dumps(request),capture_output=True,text=True,encoding='utf-8',timeout=120)
    assert verified.returncode==0,(name,recovered,verified.stderr)
    assert hashlib.sha256((BASE/name).read_bytes()).hexdigest()==case['sha256']
    results.append({'file':name,'mode':mode,'hashcatExit':run.returncode,'independentHeaderVerification':json.loads(verified.stdout)['method']});print('PASS real Hashcat recovery + independent header verification:',name,flush=True)
(OUT/'results.json').write_text(json.dumps(results,indent=2)+'\n')

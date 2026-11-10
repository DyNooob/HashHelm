"""Optional native-engine test. Uses a public MD5 target and a bounded mask run."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[1]
TARGET=Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))
CHECK=TARGET/'release'/('session-check.exe' if os.name=='nt' else 'session-check')
ENGINE=Path(os.environ.get('HASHHELM_TEST_ENGINE',str(ROOT/'vendor/hashcat-8.0.0'/('hashcat.exe' if os.name=='nt' else 'hashcat.bin'))))
OUT=ROOT/'tests/artifacts/session-recovery'/str(time.time_ns())
OUT.mkdir(parents=True)
(OUT/'cache').mkdir()
def call(**request):
    result=subprocess.run([str(CHECK)],input=json.dumps({'engine':str(ENGINE),'data':str(OUT),**request}),text=True,capture_output=True,encoding='utf8',timeout=100)
    assert result.returncode==0,result.stderr
    return [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
def main():
    target=OUT/'public.hash';target.write_text(hashlib.md5(b'public-fixture-outside-the-twelve-digit-mask').hexdigest()+'\n')
    restore=OUT/'bounded.restore'
    args=['-m','0','-a','3','--session','public-resume','--restore-file-path',str(restore),'--cache-path',str(OUT/'cache'),'--potfile-disable','--logfile-disable','--backend-ignore-cuda','-S','--status','--status-json','--status-timer','1','--runtime','5',str(target),'?d'*12]
    first=call(op='run',args=args);assert first[-1]['type']=='finished' and first[-1]['code']==4,first[-1]
    position=call(op='inspect',path=str(restore))[0];assert int(position['position'])>0,position
    assert position['trackedInputs']>=1
    resumed=args[:];resumed[resumed.index('--runtime')+1]='3';resumed+=['--restore-position']
    second=call(op='run',args=resumed);assert second[-1]['code']==4,second[-1]
    later=call(op='inspect',path=str(restore))[0];assert int(later['position'])>int(position['position']),(position,later)
    target.write_text('changed public fixture input\n')
    changed=call(op='run',args=resumed);assert changed[-1]['type']=='error' and '输入文件已更改' in changed[-1]['message'],changed
    total=call(op='probe',args=['-a','3','?d?d?d?d'],action='total')[0]['output'];assert total.strip()=='10000',total
    base=call(op='probe',args=['-a','3','?d?d?d?d'],action='keyspace')[0]['output'];assert base.strip().isdigit(),base
    lookup=call(op='probe',args=['-a','3','?d?d?d?d'],action='lookup',password='1234')[0]['output'];assert 'reaches it' in lookup,lookup
    hints=OUT/'hints.txt';hints.write_text('Jenny\n2000\n0103\n',encoding='utf8')
    pcfg=call(op='probe',args=['-a','4','hints','hintfile='+str(hints),'mask=?u?l?l?l?l?d?d?d?d','pwmin=9','pwmax=9','omen=0'],action='lookup',password='Jenny2000')[0]['output']
    assert 'reaches it' in pcfg,pcfg
    report={'checkpointBefore':position,'checkpointAfter':later,'changedInputRejected':True,'totalCandidates':total.strip(),'baseKeyspace':base.strip(),'lookup':lookup,'pcfgHintsLookup':pcfg,'limitations':['Metadata checks use size and modification time, not full content hashing.','This is a runtime-limit interruption and resume test; terminal pause/checkpoint keys are not implemented.']}
    (OUT.parent/'results.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    print('PASS: real checkpoint advances after resume; edited input rejected; native total/keyspace/lookup and PCFG hints verified')
if __name__=='__main__':main()

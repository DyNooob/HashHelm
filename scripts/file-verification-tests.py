"""End-to-end isolated verifier tests; passwords are public test data, sent on stdin."""
import hashlib, io, json, os, subprocess, sys, tempfile, zipfile
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1];BASE=ROOT/'tests/fixtures/import'
BIN=ROOT/'target'/os.environ.get('HASHHELM_TEST_PROFILE','debug')/('verify-check.exe' if os.name=='nt' else 'verify-check')
EXTRACT=BIN.with_name('extract-check.exe' if os.name=='nt' else 'extract-check')
ARTIFACTS=ROOT/'tests/artifacts/file-verification';ARTIFACTS.mkdir(parents=True,exist_ok=True)

def call(source,password,destination=None,record=None):
    if record is None:
        info=subprocess.run([str(EXTRACT),str(source)],capture_output=True,text=True,encoding='utf8',check=True)
        info=json.loads(info.stdout)
        record={'source':str(source),'sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'size':source.stat().st_size,'member':info['selected'],'format':info['format']}
    request={'record':record,'password_hex':password.encode().hex(),'destination':str(destination) if destination else None}
    run=subprocess.run([str(BIN)],input=json.dumps(request),capture_output=True,text=True,encoding='utf8',timeout=130)
    return run,record

def main():
    cases={name:'ForenHC2026!' for name in ['aes.zip','legacy.zip','stored.zip','zip64.zip','word.docx','excel.xlsx','slides.pptx','pdf-40.pdf','pdf-rc4.pdf','pdf-128.pdf','pdf-r5.pdf','pdf-256.pdf','seven.7z','seven-headers.7z','seven-store.7z','seven-lzma.7z','rar5.rar','rar5-headers.rar']}
    cases.update({'office-2007.docx':'Password1234_','office-2010.xlsx':'Password1234_','old-word.doc':'Password1234_','old-excel.xls':'Password1234_','reference.docx':'Password1234_','rar3.rar':'unrar','rar3-headers.rar':'unrar','rar3-store.rar':'unrar'})
    if len(sys.argv)>1:cases={name:cases[name] for name in sys.argv[1:]}
    results=[]
    with tempfile.TemporaryDirectory(prefix='case-',dir=ARTIFACTS) as temp:
        temp=Path(temp)
        for name,password in cases.items():
            source=BASE/name;before=hashlib.sha256(source.read_bytes()).hexdigest()
            bad,_=call(source,'DefinitelyIncorrect!');assert bad.returncode!=0,(name,'wrong password accepted')
            good,record=call(source,password)
            assert good.returncode==0,(name,good.stderr)
            receipt=json.loads(good.stdout);assert receipt['status']=='verified'
            dest=temp/(name+'.decrypted')
            exported,_=call(source,password,dest,record);assert exported.returncode==0,(name,exported.stderr)
            assert dest.stat().st_size>0
            if name.startswith(('seven','rar')):
                with zipfile.ZipFile(dest) as z:assert z.testzip() is None and len(z.namelist())>=1
            original=dest.read_bytes();again,_=call(source,password,dest,record);assert again.returncode!=0 and dest.read_bytes()==original
            overwrite,_=call(source,password,source,record);assert overwrite.returncode!=0
            assert before==hashlib.sha256(source.read_bytes()).hexdigest()
            assert password not in good.stdout and password.encode().hex() not in good.stdout
            results.append({'file':name,'wrongPasswordRejected':True,'verified':True,'exported':True,'sourceUnchanged':True,'noOverwrite':True})
            print(name,'PASS',flush=True)
        changed=temp/'changed.zip';changed.write_bytes((BASE/'legacy.zip').read_bytes());_,record=call(changed,'ForenHC2026!');changed.write_bytes(changed.read_bytes()+b'changed')
        run,_=call(changed,'ForenHC2026!',record=record);assert run.returncode!=0 and '改变' in run.stderr
        assert not list(temp.glob('.hashhelm-export-*')),'Temporary exports leaked'
    (ARTIFACTS/'results.json').write_text(json.dumps(results,ensure_ascii=False,indent=2),encoding='utf8')
    print('Source-change guard, wrong passwords, independent verification, safe export and no-overwrite passed.')
if __name__=='__main__':main()

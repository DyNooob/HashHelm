"""Native verification against public Hashcat headers and independent OpenSSL fixtures."""
from pathlib import Path
import json,subprocess,hashlib,shutil,os

ROOT=Path(__file__).resolve().parents[1]
BASE=ROOT/'tests/fixtures/containers'
ART=ROOT/'tests/artifacts/containers';ART.mkdir(parents=True,exist_ok=True)
PROFILE=os.environ.get('HASHHELM_TEST_PROFILE','release')
SUFFIX='.exe' if os.name=='nt' else ''
IMPORT=ROOT/f'target/{PROFILE}/container-check{SUFFIX}'
VERIFY=ROOT/f'target/{PROFILE}/verify-check{SUFFIX}'

def imported(path,spec):
    options=ART/'options.json';options.write_text(json.dumps(spec),encoding='utf-8')
    p=subprocess.run([str(IMPORT),str(path),str(options)],capture_output=True,text=True,encoding='utf-8',timeout=20)
    assert p.returncode==0,p.stderr
    return json.loads(p.stdout)

def verify(record,password,ok=True,destination=None):
    p=subprocess.run([str(VERIFY)],input=json.dumps({'record':record,'password_hex':password.encode().hex(),'destination':str(destination) if destination else None}),capture_output=True,text=True,encoding='utf-8',timeout=120)
    assert (p.returncode==0)==ok,(p.stdout,p.stderr)
    if ok:
        receipt=json.loads(p.stdout);assert receipt['status']=='verified';assert receipt['fingerprintScope']=='container_header';assert receipt['bytes']==512
        return receipt

def main():
    passed=[]
    for f in json.loads((BASE/'manifest.json').read_text()):
        mode=f['mode'];prf={1:'ripemd160',2:'sha512',3:'whirlpool',5:'sha256',7:'streebog',9:'blake2s'}[(mode%100)//10]
        spec={'prf':prf,'bits':mode%10*512,'header':'primary','pim_start':f.get('pim',0),'pim_stop':f.get('pim',0),'keyfiles':[str(BASE/f['keyfile'])] if f.get('keyfile') else []}
        path=BASE/f['file'];before=hashlib.sha256(path.read_bytes()).hexdigest();assert before==f['sha256']
        data=imported(path,spec);assert data['extracted']['hash'].startswith('$veracrypt$');assert data['extracted']['mode']==mode
        verify(data['record'],'wrong-public-password',False);receipt=verify(data['record'],f['password']);assert path.read_bytes() and hashlib.sha256(path.read_bytes()).hexdigest()==before
        passed.append({'file':f['file'],'mode':mode,'method':receipt['method']});print('PASS',f['file'],receipt['method'])
    for location,pw in [('primary','outer-public'),('hidden','hidden-public'),('backup','outer-public'),('hidden-backup','hidden-public')]:
        record=imported(BASE/'layout.hc',{'prf':'sha512','bits':512,'header':location,'pim_start':1,'pim_stop':1})['record'];verify(record,pw);verify(record,'other-public',False);passed.append({'header':location});print('PASS header location',location)
    source=ART/'changed.hc';shutil.copyfile(BASE/'pim-one.hc',source);record=imported(source,{'prf':'sha512','bits':512,'header':'primary','pim_start':1,'pim_stop':1})['record']
    data=bytearray(source.read_bytes());data[0]^=1;source.write_bytes(data);verify(record,'HashHelm-container-test',False)
    record=imported(BASE/'pim-one.hc',{'prf':'sha512','bits':512,'header':'primary','pim_start':1,'pim_stop':5})['record'];verify(record,'HashHelm-container-test   (PIM=1)');verify(record,'HashHelm-container-test   (PIM=6)',False)
    output=ART/'must-not-export.zip';assert not output.exists();verify(record,'HashHelm-container-test   (PIM=1)',False,output);assert not output.exists()
    key=ART/'changed-key.bin';shutil.copyfile(BASE/'public-keyfile.txt',key);record=imported(BASE/'keyfile-pim-one.hc',{'prf':'sha512','bits':512,'header':'primary','pim_start':1,'pim_stop':1,'keyfiles':[str(key)]})['record'];key.write_bytes(b'changed public fixture');verify(record,'HashHelm-container-test',False)
    (ART/'results.json').write_text(json.dumps({'passed':passed,'guards':['wrong passwords','source/keyfile changes','PIM annotation and range','header-only receipt','no decrypted-volume export'],'passwords':'public synthetic fixtures only'},indent=2)+'\n')
    print('PASS header/keyfile changes, PIM range annotations and explicit header-only export boundary')

if __name__=='__main__':main()

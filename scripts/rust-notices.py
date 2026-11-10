"""Collect exact crate notices for the native build, deduplicating identical texts."""
import hashlib,json,subprocess
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]
def main():
    host=next(line.split(': ',1)[1] for line in subprocess.check_output(['rustc','-vV'],text=True).splitlines() if line.startswith('host: '))
    meta=json.loads(subprocess.check_output(['cargo','metadata','--locked','--offline','--filter-platform',host,'--format-version','1'],cwd=ROOT,text=True,encoding='utf8'))
    texts={};packages=[]
    for package in sorted(meta['packages'],key=lambda p:(p['name'],p['version'])):
        if package['name']=='hashhelm':continue
        directory=Path(package['manifest_path']).parent
        files=[p for p in directory.iterdir() if p.is_file() and p.name.lower().startswith(('license','copying','notice','copyright'))]
        if package.get('license_file'):
            file=directory/package['license_file']
            if file.is_file() and file not in files:files.append(file)
        refs=[]
        for file in sorted(files):
            text=file.read_text(encoding='utf8',errors='replace').replace('\r\n','\n')
            identifier=hashlib.sha256(text.encode()).hexdigest()
            texts.setdefault(identifier,text);refs.append(file.name+': '+identifier)
        packages.append(package['name']+' '+package['version']+'\nLicense: '+str(package.get('license') or 'See component license')+'\nRepository: '+str(package.get('repository') or '')+'\n'+'\n'.join(refs))
    output=ROOT/'assets/licenses/Rust-Dependency-Notices.txt'
    header='# Rust dependency license notices\n\nNative target: '+host+'\nExact versions and checksums: Cargo.lock. HashHelm has its own LICENSE.\nIdentical notice texts are included once and referenced by SHA-256.\nThe UnRAR core has an additional license in native-verification/UnRAR-license.txt.\n\n'
    output.write_text(header+'\n\n'.join(packages)+'\n\n'+''.join('\n'+'='*72+'\nTEXT '+key+'\n'+value+'\n' for key,value in texts.items()),encoding='utf8')
    print(json.dumps({'target':host,'crates':len(packages),'uniqueTexts':len(texts),'bytes':output.stat().st_size}))
if __name__=='__main__':main()

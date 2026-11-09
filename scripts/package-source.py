"""Export a reviewable source bundle, excluding local evidence and run data."""
import hashlib
import json
from pathlib import Path
import tomllib
import zipfile

ROOT=Path(__file__).resolve().parents[1]
VERSION=tomllib.loads((ROOT/'Cargo.toml').read_text())['package']['version']
DIRECTORIES=('src','assets','.github','tests/fixtures')
FILES=('ui/index.html','ui/style.css','ui/core.js','ui/i18n.js','ui/app.js','Cargo.toml','Cargo.lock','build.rs','README.md','BUILDING.md','GUIDE.zh-CN.md','LICENSE','CREDITS.md','THIRD_PARTY.md','.gitattributes','.gitignore','tests/test_packaging.py','tests/browser/package.json','tests/browser/package-lock.json')
SCRIPTS=('build-hashcat.sh','build-unix.sh','package_unix.py','package-source.py','native_startup.py','install-desktop.py','ui-tests.cjs','workflow-tests.mjs','platform-ui-tests.mjs','package.ps1','run-dev.ps1')

def main():
    files={ROOT/name for name in FILES}
    files.update(ROOT/'scripts'/name for name in SCRIPTS)
    for name in DIRECTORIES:files.update(p for p in (ROOT/name).rglob('*') if p.is_file() and '__pycache__' not in p.parts)
    prefix=f'HashHelm-{VERSION}-source'
    manifest=[];output=ROOT/'dist'/f'{prefix}.zip';output.parent.mkdir(exist_ok=True)
    with zipfile.ZipFile(output,'w',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
        for file in sorted(files):
            relative=file.relative_to(ROOT).as_posix();data=file.read_bytes()
            if file.suffix in ('.sh','.yml','.rs','.py'):data=data.replace(b'\r\n',b'\n')
            info=zipfile.ZipInfo(prefix+'/'+relative);info.create_system=3;info.external_attr=(0o100755 if file.suffix=='.sh' else 0o100644)<<16;info.compress_type=zipfile.ZIP_DEFLATED
            archive.writestr(info,data)
            manifest.append({'path':relative,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
        archive.writestr(prefix+'/SOURCE-CONTENTS.json',json.dumps(manifest,ensure_ascii=False,indent=2))
    with zipfile.ZipFile(output) as archive:assert archive.testzip() is None
    report={'version':VERSION,'kind':'source; native executables are not included','bytes':output.stat().st_size,'files':len(manifest),'sha256':hashlib.sha256(output.read_bytes()).hexdigest()}
    output.with_suffix('.json').write_text(json.dumps(report,indent=2)+'\n')
    print(str(output));print(json.dumps(report,indent=2))

if __name__=='__main__':main()

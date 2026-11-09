"""Native macOS/Linux packaging. No downloads, root access, or system installation."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import struct
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
VERSION = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
RUNTIME_DIRS = ('OpenCL', 'modules', 'bridges', 'feeds', 'charsets', 'docs', 'extra', 'masks', 'rules', 'tables', 'tunings', 'pcfg', 'Python', 'Rust', 'tools', 'cache')
DOCS = ('README.md', 'GUIDE.zh-CN.md', 'BUILDING.md', 'THIRD_PARTY.md', 'LICENSE', 'CREDITS.md')
IGNORED = {'.git', 'obj', 'target', '__pycache__', 'workspace', 'webview'}


def architectures(path):
    with Path(path).open('rb') as stream:
        head = stream.read(4096)
    if head[:4] == b'\x7fELF' and len(head) >= 20:
        endian = '<' if head[5] == 1 else '>'
        return 'linux', {62: 'x64', 183: 'arm64'}.get(struct.unpack_from(endian + 'H', head, 18)[0], 'unknown').split()
    magic = head[:4]
    if magic in (b'\xcf\xfa\xed\xfe', b'\xfe\xed\xfa\xcf'):
        cpu = struct.unpack_from(('<' if magic[0] == 0xcf else '>') + 'I', head, 4)[0]
        return 'macos', [{0x01000007: 'x64', 0x0100000c: 'arm64'}.get(cpu, 'unknown')]
    if magic in (b'\xca\xfe\xba\xbe', b'\xca\xfe\xba\xbf'):
        count = struct.unpack_from('>I', head, 4)[0]
        if count > 16: raise ValueError('Invalid universal Mach-O header')
        width = 32 if magic[-1] == 0xbf else 20
        return 'macos', [{0x01000007: 'x64', 0x0100000c: 'arm64'}.get(struct.unpack_from('>I', head, 8 + width * i)[0], 'unknown') for i in range(count)]
    raise ValueError(f'{path} is not an ELF/Mach-O binary (Windows EXEs cannot be repackaged for Unix)')


def validate_engine(folder, os_name, arch):
    names = ('hashcat.bin', 'hashcat') if os_name == 'linux' else ('hashcat', 'hashcat.bin')
    binary = next((folder / name for name in names if (folder / name).is_file()), None)
    if binary is None: raise ValueError('Build a native Hashcat runtime first')
    binary_os, arches = architectures(binary)
    if binary_os != os_name or arch not in arches: raise ValueError('Hashcat architecture does not match the GUI')
    library = folder / ('libhashcat.8.dylib' if os_name == 'macos' else 'libhashcat.so.8')
    if not library.is_file(): raise ValueError(f'Missing Hashcat 8 shared core: {library}')
    for name in ('modules', 'OpenCL', 'rules', 'charsets', 'pcfg', 'bridges', 'feeds'):
        if not (folder / name).is_dir(): raise ValueError(f'Incomplete runtime: {name}')
    native_modules = [p for p in (folder / 'modules').glob('module_*') if p.suffix in ('.so', '.dylib')]
    if len(native_modules) < 500: raise ValueError('The full module set was not built')
    if not any(p.suffix in ('.so', '.dylib') for p in (folder / 'bridges').glob('bridge_rust_generic_hash.*')) or not any(p.suffix in ('.so', '.dylib') for p in (folder / 'bridges/subs').glob('*generic_hash*')):
        raise ValueError('Missing the Rust bridge or its default plugin; install libclang and rebuild the full engine')
    version = subprocess.run([str(binary), '--version'], cwd=folder, capture_output=True, text=True, timeout=20, check=True).stdout.strip()
    if version != 'v8.0.0': raise ValueError(f'Expected Hashcat v8.0.0, found {version}')
    return binary


def copy_engine(source, destination):
    destination.mkdir()
    def ignored(folder, names):
        return [name for name in names if name in IGNORED or name.endswith(('.o', '.log', '.pyc', '.dSYM', '.exe', '.dll'))]
    for name in RUNTIME_DIRS:
        folder = source / name
        if folder.is_dir(): shutil.copytree(folder, destination / name, ignore=ignored, symlinks=False)
    for file in source.iterdir():
        if file.is_file() and (file.name in ('hashcat', 'hashcat.bin', 'hashcat.hcstat2') or file.name.startswith('libhashcat') or file.name.startswith(('README', 'LICENSE', 'example')) and file.suffix not in ('.cmd', '.log')):
            shutil.copy2(file, destination / file.name)
    for file in destination.rglob('*'):
        if file.is_file() and (file.name in ('hashcat', 'hashcat.bin') or file.suffix in ('.so', '.dylib', '.sh') or file.name.startswith('libhashcat')):
            file.chmod(file.stat().st_mode | 0o111)


def mac_icon(destination):
    with tempfile.TemporaryDirectory() as temp:
        iconset = Path(temp) / 'DigiForensics.iconset'
        iconset.mkdir()
        for size in (16, 32, 128, 256, 512):
            for scale in (1, 2):
                output = iconset / f'icon_{size}x{size}{"@2x" if scale == 2 else ""}.png'
                subprocess.run(['sips', '-z', str(size * scale), str(size * scale), str(ROOT / 'assets/branding/digiforensics.png'), '--out', str(output)], check=True, stdout=subprocess.DEVNULL)
        subprocess.run(['iconutil', '-c', 'icns', str(iconset), '-o', str(destination)], check=True)


def add_tree(tar, directory, prefix):
    tar.add(directory, arcname=prefix, recursive=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--engine-dir', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/hashhelm')
    parser.add_argument('--output', type=Path, default=ROOT / 'dist')
    args = parser.parse_args()
    os_name = {'Darwin': 'macos', 'Linux': 'linux'}.get(platform.system())
    if not os_name: parser.error('Run this on macOS or Linux; Windows cannot create a verified native package')
    arch = {'arm64': 'arm64', 'aarch64': 'arm64', 'x86_64': 'x64', 'AMD64': 'x64'}.get(platform.machine())
    if not arch: parser.error('Supported architectures: arm64 and x86_64')
    binary = args.binary.resolve(strict=True)
    binary_os, arches = architectures(binary)
    if binary_os != os_name or arch not in arches: parser.error('GUI is not native to this build host')
    source = args.engine_dir.resolve(strict=True)
    validate_engine(source, os_name, arch)
    args.output.mkdir(parents=True, exist_ok=True)
    name = f'HashHelm-{VERSION}-{os_name}-{arch}'
    # Never merge with a previous release: stale files could hide missing runtime assets.
    stage = args.output / name
    if stage.exists(): parser.error(f'{stage} already exists; choose a fresh --output directory')
    stage.mkdir()
    if os_name == 'macos':
        contents = stage / 'HashHelm.app/Contents'
        resources = contents / 'Resources'
        executable = contents / 'MacOS/HashHelm'
        resources.mkdir(parents=True)
        executable.parent.mkdir()
        info = {'CFBundleName': 'HashHelm', 'CFBundleDisplayName': 'HashHelm', 'CFBundleIdentifier': 'com.digiforensics.hashhelm', 'CFBundleExecutable': 'HashHelm', 'CFBundlePackageType': 'APPL', 'CFBundleShortVersionString': VERSION, 'CFBundleVersion': VERSION, 'CFBundleIconFile': 'DigiForensics.icns', 'LSMinimumSystemVersion': '14.0', 'NSHighResolutionCapable': True, 'NSHumanReadableCopyright': 'Copyright (c) 2026 DyNooob · HashHelm Attribution License v1.0'}
        (contents / 'Info.plist').write_bytes(plistlib.dumps(info))
        mac_icon(resources / 'DigiForensics.icns')
    else:
        resources = stage
        executable = stage / 'HashHelm'
        shutil.copy2(ROOT / 'scripts/install-desktop.py', stage / 'install-desktop.py')
        launcher = stage / 'launch.sh'
        launcher.write_text('#!/bin/sh\nset -eu\nbase=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)\nexec "$base/HashHelm" "$@"\n', encoding='utf-8')
        launcher.chmod(0o755)
    shutil.copy2(binary, executable)
    executable.chmod(0o755)
    shutil.copytree(ROOT / 'assets', resources / 'assets')
    for doc in DOCS: shutil.copy2(ROOT / doc, resources / doc)
    engine = resources / 'hashcat'
    copy_engine(source, engine)
    if os_name == 'macos':
        for file in engine.rglob('*'):
            if file.is_file() and (file.suffix in ('.dylib', '.so') or file.name in ('hashcat', 'hashcat.bin')):
                subprocess.run(['codesign', '--force', '--sign', '-', str(file)], check=True, capture_output=True)
        subprocess.run(['codesign', '--force', '--sign', '-', str(stage / 'HashHelm.app')], check=True, capture_output=True)
    metadata = json.loads(subprocess.run([str(executable), '--hashhelm-platform-info'], capture_output=True, text=True, check=True, timeout=20).stdout)
    if metadata['platform'] != os_name or metadata['version'] != VERSION: raise ValueError('Packaged host failed its platform check')
    validate_engine(engine, os_name, arch)
    engine_bytes = sum(p.stat().st_size for p in engine.rglob('*') if p.is_file())
    total = sum(p.stat().st_size for p in stage.rglob('*') if p.is_file())
    if total - engine_bytes > 10_000_000: raise ValueError('Static UI overhead exceeds 10 MB')
    archive = args.output / f'{name}.tar.gz'
    with tarfile.open(archive, 'w:gz', compresslevel=9) as tar: add_tree(tar, stage, name)
    with tempfile.TemporaryDirectory(dir=args.output) as temp:
        baseline = Path(temp) / 'engine.tar.gz'
        with tarfile.open(baseline, 'w:gz', compresslevel=9) as tar: add_tree(tar, engine, f'{name}/hashcat')
        overhead = archive.stat().st_size - baseline.stat().st_size
        if overhead > 10_000_000: raise ValueError('Compressed UI overhead exceeds 10 MB')
    report = {'version': VERSION, 'platform': os_name, 'arch': arch, 'archiveBytes': archive.stat().st_size, 'engineBytes': engine_bytes, 'staticOverheadBytes': total - engine_bytes, 'archiveOverheadBytes': overhead, 'sha256': hashlib.sha256(archive.read_bytes()).hexdigest(), 'signed': 'ad-hoc development signature; not notarized' if os_name == 'macos' else False}
    (args.output / f'{name}.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(report, indent=2))


if __name__ == '__main__': main()

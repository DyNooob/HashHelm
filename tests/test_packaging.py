"""Packaging format and completeness guards, without pretending to build native apps."""
import importlib.util
from pathlib import Path
import struct
import tempfile
import uuid
import shutil
from contextlib import contextmanager
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('package_unix',ROOT/'scripts/package_unix.py')
pack=importlib.util.module_from_spec(spec)
spec.loader.exec_module(pack)

def elf(arch=62):
    head=bytearray(64);head[:6]=b'\x7fELF\x02\x01';struct.pack_into('<H',head,18,arch);return head

@contextmanager
def scratch():
    base=(ROOT/'tests/artifacts/packaging').resolve();base.mkdir(parents=True,exist_ok=True)
    folder=(base/uuid.uuid4().hex).resolve();assert folder.is_relative_to(base)
    folder.mkdir()
    try:yield str(folder)
    finally:
        assert folder.resolve().is_relative_to(base) and not folder.is_symlink()
        shutil.rmtree(folder)

class PackagingTests(unittest.TestCase):
    def test_formats_and_architectures(self):
        with scratch() as temp:
            path=Path(temp)/'binary'
            path.write_bytes(elf());self.assertEqual(pack.architectures(path),('linux',['x64']))
            path.write_bytes(elf(183));self.assertEqual(pack.architectures(path),('linux',['arm64']))
            path.write_bytes(b'\xcf\xfa\xed\xfe'+struct.pack('<I',0x0100000c)+bytes(24));self.assertEqual(pack.architectures(path),('macos',['arm64']))
            path.write_bytes(b'MZ'+bytes(64))
            with self.assertRaises(ValueError):pack.architectures(path)

    def test_missing_core_or_modules_cannot_ship(self):
        with scratch() as temp:
            folder=Path(temp);(folder/'hashcat').write_bytes(elf())
            with self.assertRaisesRegex(ValueError,'shared core'):pack.validate_engine(folder,'linux','x64')
            (folder/'libhashcat.so.8').write_bytes(elf())
            for name in ('modules','OpenCL','rules','charsets','pcfg','bridges','feeds'):(folder/name).mkdir()
            with self.assertRaisesRegex(ValueError,'full module set'):pack.validate_engine(folder,'linux','x64')
            with self.assertRaisesRegex(ValueError,'architecture'):pack.validate_engine(folder,'macos','arm64')
            for i in range(500):(folder/'modules'/f'module_{i:05}.so').write_bytes(elf())
            with self.assertRaisesRegex(ValueError,'Rust bridge'):pack.validate_engine(folder,'linux','x64')

    def test_runtime_assets_survive_without_caches_or_build_objects(self):
        with scratch() as temp:
            source=Path(temp)/'source';source.mkdir();target=Path(temp)/'runtime'
            for name in pack.RUNTIME_DIRS:
                (source/name).mkdir();(source/name/'keep.txt').write_text(name)
            (source/'bridges/target').mkdir();(source/'bridges/target/scratch.o').write_text('object')
            (source/'hashcat').write_bytes(elf());(source/'libhashcat.so.8').write_bytes(elf());(source/'hashcat.exe').write_bytes(b'MZ')
            (source/'hashcat.hcstat2').write_bytes(b'data')
            (source/'cache').mkdir();(source/'cache/module_options.so.cache').write_text('build-host cache')
            pack.copy_engine(source,target)
            for name in pack.RUNTIME_DIRS:self.assertTrue((target/name/'keep.txt').is_file())
            self.assertTrue((target/'libhashcat.so.8').is_file());self.assertFalse((target/'hashcat.exe').exists());self.assertFalse((target/'bridges/target').exists());self.assertFalse((target/'cache').exists())

    def test_native_embedded_page_stays_below_webview2_string_limit(self):
        import base64
        import html
        page=(ROOT/'ui/index.html').read_text(encoding='utf-8')
        self.assertEqual(page.count('/* LOGO */'),1,'Reuse the embedded logo instead of duplicating its base64 payload')
        for marker,file in [('STYLES','style.css'),('CORE','core.js'),('I18N','i18n.js'),('WORKBENCH','workbench.js'),('UPDATES','updates.js'),('TOOLS','tools.js'),('APP','app.js')]:
            page=page.replace('/* '+marker+' */',(ROOT/'ui'/file).read_text(encoding='utf-8'))
        page=page.replace('<!-- LICENSE -->',html.escape((ROOT/'LICENSE').read_text(encoding='utf-8'),quote=False))
        page=page.replace('<!-- WEBVIEW-LICENSE -->',html.escape((ROOT/'assets/licenses/WebView2-Fixed-Version-License.txt').read_text(encoding='utf-8'),quote=False))
        page=page.replace('/* LOGO */',base64.b64encode((ROOT/'assets/branding/digiforensics.ico').read_bytes()).decode())
        self.assertLess(len(page.encode('utf-8')),2*1024*1024,'WebView2 NavigateToString rejects oversized inline pages')

if __name__=='__main__':unittest.main()

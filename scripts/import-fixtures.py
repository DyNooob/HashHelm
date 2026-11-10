"""Generate non-sensitive encrypted fixtures. Python is a test-only dependency."""
import io, sys, zipfile, zlib, struct
from pathlib import Path
sys.path.insert(0, str(Path('tests/python-deps').resolve()))
import pyzipper, msoffcrypto
from pypdf import PdfWriter
from pypdf.generic import ArrayObject, ByteStringObject
base=Path('tests/fixtures/import');base.mkdir(parents=True,exist_ok=True)
# Historical fixture password: keep it in sync with the already-encrypted test files.
password='ForenHC2026!'
with pyzipper.AESZipFile(base/'aes.zip','w',compression=zipfile.ZIP_DEFLATED,encryption=pyzipper.WZ_AES) as z:
    z.setpassword(password.encode());z.writestr('example.txt','ForenHC encrypted example.');z.writestr('second.txt','Another encrypted member.')

def legacy_zip(path,method=8):
    plain=b'ForenHC ZIP test: '+b'abcde'*20
    data=zlib.compress(plain,wbits=-15) if method==8 else plain
    crc=zlib.crc32(plain);name=b'test.txt'
    keys=[0x12345678,0x23456789,0x34567890]
    def crcbyte(c,k):
        c^=k&255
        for _ in range(8):c=(c>>1)^ (0xedb88320 if c&1 else 0)
        return ((k>>8)^c)&0xffffffff
    def update(c):
        keys[0]=crcbyte(c,keys[0]);keys[1]=((keys[1]+(keys[0]&255))*134775813+1)&0xffffffff;keys[2]=crcbyte(keys[1]>>24,keys[2])
    for c in password.encode():update(c)
    encrypted=bytearray()
    for c in b'\0'*10+struct.pack('<H',crc>>16)+data:
        k=(keys[2]|2)&65535;encrypted.append(c^((k*(k^1)>>8)&255));update(c)
    local=struct.pack('<4s5H3I2H',b'PK\x03\x04',20,1,method,0,0,crc,len(encrypted),len(plain),len(name),0)+name+encrypted
    central=struct.pack('<4s6H3I5H2I',b'PK\x01\x02',20,20,1,method,0,0,crc,len(encrypted),len(plain),len(name),0,0,0,0,0,0)+name
    end=struct.pack('<4s4H2IH',b'PK\x05\x06',0,0,1,1,len(central),len(local),0)
    path.write_bytes(local+central+end)
    with zipfile.ZipFile(path) as z:assert z.read('test.txt',pwd=password.encode())==plain
legacy_zip(base/'legacy.zip');legacy_zip(base/'stored.zip',0)

types={
 'word.docx':('word/document.xml','application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml','<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>ForenHC test</w:t></w:r></w:p></w:body></w:document>'),
 'excel.xlsx':('xl/workbook.xml','application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml','<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheets/></workbook>'),
 'slides.pptx':('ppt/presentation.xml','application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml','<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"/>')}
for name,(target,content_type,content) in types.items():
    buf=io.BytesIO()
    with zipfile.ZipFile(buf,'w') as z:
        z.writestr('[Content_Types].xml',f'<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/{target}" ContentType="{content_type}"/></Types>')
        z.writestr('_rels/.rels',f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="{target}"/></Relationships>')
        z.writestr(target,content)
        z.writestr('padding.bin', b'0'*5000)
    (base/('plain-'+name)).write_bytes(buf.getvalue());buf.seek(0)
    office=msoffcrypto.OfficeFile(buf)
    with (base/name).open('wb') as out:office.encrypt(password,out)
    with (base/name).open('rb') as f:
        office=msoffcrypto.OfficeFile(f);office.load_key(password=password,verify_password=True);out=io.BytesIO();office.decrypt(out);assert out.getvalue()==buf.getvalue()
for name,algorithm in [('pdf-128.pdf','AES-128'),('pdf-256.pdf','AES-256'),('pdf-40.pdf','RC4-40'),('pdf-rc4.pdf','RC4-128'),('pdf-r5.pdf','AES-256-R5')]:
    writer=PdfWriter();writer.add_blank_page(width=200,height=200)
    if algorithm=='RC4-40':writer._ID=ArrayObject([ByteStringObject(b'0123456789abcdef'),ByteStringObject(b'0123456789abcdef')])
    writer.encrypt(password,owner_password='Owner-Only-Example',algorithm=algorithm)
    with (base/name).open('wb') as f:writer.write(f)
(base/'passwords.txt').write_text('wrong\n'+password+'\n',encoding='utf-8')
(base/'truncated.zip').write_bytes(b'PK\x03\x04'+b'\0'*15)
(base/'fake.docx').write_bytes(b'This is not an Office file')
print('Generated ZIP, Office and PDF encrypted fixtures; password:',password)

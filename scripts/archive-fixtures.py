"""Generate synthetic native archive cases; 7-Zip/WinRAR are test tools only."""
import hashlib, json, shutil, struct, subprocess, sys, tempfile, zipfile, zlib
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]
BASE=ROOT/'tests/fixtures/import'
PASSWORD='ForenHC2026!'

def zip64():
    data=(BASE/'legacy.zip').read_bytes()
    pos=data.index(b'PK\x01\x02');end=data.index(b'PK\x05\x06')
    central=bytearray(data[pos:end]);packed,plain=struct.unpack_from('<II',central,20)
    offset=struct.unpack_from('<I',central,42)[0]
    name_size=struct.unpack_from('<H',central,28)[0]
    extra=struct.pack('<HHQQQ',1,24,plain,packed,offset)
    struct.pack_into('<H',central,6,45)
    struct.pack_into('<II',central,20,0xffffffff,0xffffffff)
    struct.pack_into('<H',central,30,len(extra));struct.pack_into('<I',central,42,0xffffffff)
    central=central[:46+name_size]+extra+central[46+name_size:]
    z64=struct.pack('<4sQHHIIQQQQ',b'PK\x06\x06',44,45,45,0,0,1,1,len(central),pos)
    locator=struct.pack('<4sIQI',b'PK\x06\x07',0,pos+len(central),1)
    eocd=struct.pack('<4s4H2IH',b'PK\x05\x06',0,0,65535,65535,0xffffffff,0xffffffff,0)
    (BASE/'zip64.zip').write_bytes(data[:pos]+central+z64+locator+eocd)
    with zipfile.ZipFile(BASE/'zip64.zip') as z:assert z.read('test.txt',pwd=PASSWORD.encode())
    damaged=bytearray((BASE/'zip64.zip').read_bytes());damaged[-42:-38]=b'FAIL'
    (BASE/'broken-zip64.zip').write_bytes(damaged)

def main():
    zip64()
    seven=shutil.which('7z') or r'C:\Program Files\7-Zip\7z.exe'
    rar=shutil.which('rar') or r'D:\Program Files\WinRAR\Rar.exe'
    staging=ROOT/'tests/artifacts';staging.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='archive-fixtures-',dir=staging) as temp:
        temp=Path(temp);(temp/'sample.txt').write_text('HashHelm synthetic archive test.\n'*30)
        (temp/'second.txt').write_text('Second file, same password.\n'*10)
        for name,flags in [('seven.7z',[]),('seven-headers.7z',['-mhe=on']),('seven-store.7z',['-mx=0']),('seven-lzma.7z',['-m0=lzma'])]:
            subprocess.run([seven,'a',str(temp/name),'sample.txt','second.txt','-p'+PASSWORD,*flags],cwd=temp,check=True,capture_output=True)
            shutil.copyfile(temp/name,BASE/name)
        for name,flags in [('rar5.rar',[]),('rar5-headers.rar',['-hp'+PASSWORD])]:
            subprocess.run([rar,'a','-ma5','-p'+PASSWORD,*flags,str(temp/name),'sample.txt','second.txt'],cwd=temp,check=True,capture_output=True)
            shutil.copyfile(temp/name,BASE/name)
    print(json.dumps({'generated':['zip64.zip','broken-zip64.zip','seven.7z','seven-headers.7z','seven-store.7z','seven-lzma.7z','rar5.rar','rar5-headers.rar']}))

def rar3_headers():
    # Transform the MIT-licensed unrar crate's tiny public RAR3 fixture.
    # Fixed salts keep this regression case reproducible; never used in product crypto.
    sys.path.insert(0,str(ROOT/'tests/python-deps'))
    from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
    data=(BASE/'rar3.rar').read_bytes();password='unrar'.encode('utf-16le')
    def encrypt(header,salt):
        sha=hashlib.sha1();iv=bytearray()
        for i in range(0x40000):
            sha.update(password+salt+i.to_bytes(3,'little'))
            if i%0x4000==0:iv.append(sha.copy().digest()[-1])
        digest=sha.digest();key=b''.join(digest[i:i+4][::-1] for i in range(0,16,4))
        block=header+b'\0'*((-len(header))%16)
        cipher=Cipher(algorithms.AES(key),modes.CBC(bytes(iv))).encryptor()
        return salt+cipher.update(block)+cipher.finalize()
    main=bytearray(data[7:20]);struct.pack_into('<H',main,3,128);struct.pack_into('<H',main,0,zlib.crc32(main[2:])&65535)
    file_size=struct.unpack_from('<H',data,25)[0];packed=struct.unpack_from('<I',data,27)[0]
    start=20;payload=start+file_size;end=payload+packed
    result=data[:7]+main+encrypt(data[start:payload],b'fixture1')+data[payload:end]+encrypt(data[end:],b'fixture2')
    (BASE/'rar3-headers.rar').write_bytes(result)
    plain=b'HashHelm synthetic stored RAR3 test.';name=b'sample.txt';salt=b'fixture3'
    cipher=encrypt(plain,salt)[8:];header=bytearray(32)
    header[2]=0x74;struct.pack_into('<HHII',header,3,0x8404,32+len(name)+8,len(cipher),len(plain))
    header[15]=3;struct.pack_into('<I',header,16,zlib.crc32(plain));header[24]=29;header[25]=0x30
    struct.pack_into('<H',header,26,len(name));struct.pack_into('<I',header,28,0x20)
    header+=name+salt;struct.pack_into('<H',header,0,zlib.crc32(header[2:])&65535)
    (BASE/'rar3-store.rar').write_bytes(data[:20]+header+cipher+data[end:])

if __name__=='__main__':
    if '--rar3-headers' in sys.argv:rar3_headers()
    else:main()

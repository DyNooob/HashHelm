# Third-party software

HashHelm original code: Copyright (c) 2026 DyNooob, licensed under the
HashHelm Attribution License v1.0 in LICENSE. This custom license does not
change any component license listed below.

Hashcat 8.0.0 was downloaded from https://hashcat.net/files/hashcat-8.0.0.7z
on 2026-11-09. Archive SHA-256:
9fa86e1bfd5dd898d3c83aa38a67b08db1ba5d2f5252dffa2af44a8c62585f49

The complete official distribution is included under `hashcat/`, including
Linux binaries, documentation, tools, dictionaries, models, and license files.
Hashcat is licensed under MIT; bundled component notices are in its `docs/`.
This UI is an independent project and is not affiliated with the Hashcat team.
The SHA-256 records this download; it is not an independently verified publisher signature.

Rust dependencies and exact versions are recorded in Cargo.lock. Key dependencies:
wry (Apache-2.0 / MIT), tao (Apache-2.0), rfd (MIT), serde (Apache-2.0 / MIT),
serde_json (Apache-2.0 / MIT). WebView2 Runtime is a Microsoft system dependency
and is included unmodified in the Windows offline package. The lite package uses the system runtime.

Native file import uses cfb (MIT), roxmltree (Apache-2.0 / MIT), base64 (Apache-2.0 / MIT) and lopdf (MIT). Exact versions and transitive dependencies are in Cargo.lock.

Verifier formats were checked against the Microsoft MS-OFFCRYPTO specification,
Hashcat module parsers, and Openwall office2john/zip2john format documentation.
No John executable, Python runtime, or Python package is bundled.
Openwall reference tools and downloaded public fixtures are development-only.
Public Office test samples originate from https://github.com/nolze/msoffcrypto-tool/tree/master/tests/inputs (MIT).


0.3 resources: dictionary files and sources/checksums are in assets/wordlists-manifest.json.
SecLists common-password lists: MIT; original Chinese Common Password List by Kyi Wong: MIT. Both license texts are included in assets/licenses/.
cfbao Chinese Diceware word data: CC BY 4.0. Chinese words were extracted and deduplicated, and the pinyin derivative was generated with the Rust pinyin crate. Attribution and modification details: assets/licenses/Chinese-Diceware-Attribution.txt.
pinyin 0.11.0: MIT. CUPP (https://github.com/Mebus/cupp) was researched as a workflow reference only; its GPL source and executables are not bundled. The native profile generator was implemented independently.

DigiForensics Logo: supplied by the user in this development session. Original PNG preserved; standard-size Windows icons and macOS ICNS resources derived for the application.

macOS uses the system WebKit framework. Linux uses system GTK3 and WebKitGTK 4.1;
these system libraries are not bundled. Unix process cleanup uses libc (MIT/Apache-2.0)
and native process groups. Native Hashcat builds pin the upstream 8.0.0 commit documented in BUILDING.md.

## Native verification components (0.8)

Independent file verification and decrypted-copy export use zip 6.0.0 (MIT),
sevenz-rust2 / lzma-rust2 0.25.0 (Apache-2.0), msoffice-crypto 0.1.0-rc.13
(MIT / Apache-2.0), unrar 0.5.8 (MIT / Apache-2.0 wrapper), sha2 0.10.9,
crc32fast and tempfile (MIT / Apache-2.0). Exact resolved versions and checksums
are in Cargo.lock. License texts and the Office NOTICE are included in
assets/licenses/native-verification/.

The unrar_sys dependency statically builds Alexander Roshal's UnRAR decompression
library. **UnRAR is subject to its own license, not HashHelm's license or the
wrapper's MIT/Apache license.** Its full license is included in
assets/licenses/native-verification/UnRAR-license.txt. In particular:

> UnRAR source code may be used in any software to handle RAR archives without limitations free of charge, but cannot be used to develop RAR (WinRAR) compatible archiver and to re-create RAR compression algorithm, which is proprietary.

HashHelm uses UnRAR for reading/testing RAR only; it does not create RAR files.
7-Zip and WinRAR installed on the development host generate synthetic test
fixtures only; their installed executables are not redistributed.

The tiny RAR3 fixture originates from the unrar 0.5.8 crate's data/crypted.rar
(https://github.com/muja/unrar.rs, MIT / Apache-2.0). The file-name encrypted
RAR3 fixture is a deterministic transformation of that sample using the same
public password, implemented in scripts/archive-fixtures.py. The original
authors' license texts are retained in the component notice directory.

Rust-Dependency-Notices.txt in assets/licenses is regenerated for each native build from Cargo.lock and the installed public crate license/notice files. Identical texts are referenced by SHA-256 and included once.

## Microsoft WebView2 Fixed Version (Windows offline package)

Acquired directly from the Microsoft download endpoint recorded in assets/webview2-runtime.json. The CAB SHA-256 is checked and msedgewebview2.exe / msedge.dll Authenticode signatures must be valid and issued to Microsoft Corporation. All runtime files and notices are preserved. Distributed only as part of HashHelm, under Microsoft's separate terms in assets/licenses/WebView2-Fixed-Version-License.html and .txt. Original HTML: https://developer.microsoft.com/microsoft-edge/api/eula/webview2?locale=en-us&fixed=true . The text is a readable rendering of the same terms. SmartScreen/privacy notice is provided in the UI, README and SECURITY-NOTES.md. Microsoft trademarks do not imply endorsement of HashHelm.

## Update client

The native Foren DEV update client uses ureq 3.5.1, rustls and rustls-platform-verifier for HTTPS with the system trust store, and semver for version precedence. Their licenses and transitive dependency notices are included in assets/licenses/Rust-Dependency-Notices.txt. No update-service administrator credentials or license signing keys are distributed.

## VeraCrypt header verification (0.11)

Native header verification uses RustCrypto aes/cipher, serpent, twofish, camellia, kuznyechik, pbkdf2/hmac, ripemd, whirlpool, streebog and blake2, under their individual licenses included in Rust-Dependency-Notices.txt. No VeraCrypt binaries, drivers or mount services are distributed. The implementation follows the public volume-format specification. Test header provenance is recorded separately in tests/fixtures/containers/manifest.json; Hashcat public examples retain Hashcat's MIT attribution.

Native macOS Hashcat 8.0.0 includes the small, reproducible patch in scripts/patch-hashcat-macos.py: PID session locks use the existing cache directory instead of the signed application's resource directory. No hash-mode or cryptographic implementation is changed. Windows and Linux engine distributions are unchanged. Upstream Hashcat remains MIT-licensed.

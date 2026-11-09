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
and is not redistributed in this package.

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

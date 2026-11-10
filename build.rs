use std::{env, fs, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-env-changed=HASHHELM_BUILD_NUMBER");
    let build = env::var("HASHHELM_BUILD_NUMBER").unwrap_or_else(|_| "1".into());
    let build: u32 = build
        .parse()
        .expect("HASHHELM_BUILD_NUMBER must be an integer");
    assert!(
        (1..=i32::MAX as u32).contains(&build),
        "Invalid build number"
    );
    println!("cargo:rustc-env=HASHHELM_BUILD_NUMBER={build}");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // UnRAR uses the Windows registry and CryptoAPI, including in headless tools.
        println!("cargo:rustc-link-lib=advapi32");
    }
    println!("cargo:rerun-if-changed=assets/branding/digiforensics.ico");
    println!("cargo:rerun-if-changed=assets/branding/windows.manifest");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || env::var_os("CARGO_FEATURE_DESKTOP").is_none()
    {
        return;
    }
    let sdk = PathBuf::from(
        env::var("ProgramFiles(x86)").unwrap_or_else(|_| "C:/Program Files (x86)".into()),
    )
    .join("Windows Kits/10/bin");
    let mut versions = fs::read_dir(sdk)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect::<Vec<_>>();
    versions.sort();
    versions.reverse();
    let rc = versions
        .into_iter()
        .map(|p| p.join("x64/rc.exe"))
        .find(|p| p.is_file())
        .expect("Windows SDK resource compiler rc.exe was not found");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let res = out.join("hashhelm.res");
    let script = out.join("hashhelm.rc");
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let mut numbers = version
        .split('.')
        .take(3)
        .map(|s| s.parse::<u16>().unwrap_or(0))
        .collect::<Vec<_>>();
    numbers.resize(4, 0);
    let numeric_version = numbers
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("assets/branding/digiforensics.ico");
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("assets/branding/windows.manifest")
        .display()
        .to_string()
        .replace('\\', "/");
    fs::write(
        &script,
        format!(
            r#"1 ICON "{}"
1 24 "{manifest}"
1 VERSIONINFO
FILEVERSION {numeric_version}
PRODUCTVERSION {numeric_version}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "DigiForensics"
      VALUE "LegalCopyright", "Copyright (c) 2026 DyNooob"
      VALUE "FileDescription", "HashHelm"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "hashhelm"
      VALUE "OriginalFilename", "HashHelm.exe"
      VALUE "ProductName", "HashHelm"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#,
            icon.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    let status = Command::new(rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&res)
        .arg(&script)
        .status()
        .expect("Could not run rc.exe");
    assert!(status.success(), "Icon resource compilation failed");
    println!("cargo:rustc-link-arg-bin=hashhelm={}", res.display());
}

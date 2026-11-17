//! Windows: Programm-Icon und Versionsangaben in die .exe einbetten (falls ein Ressourcen-Compiler da ist).

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/amboss.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icon/amboss.ico");
    let ver = env!("CARGO_PKG_VERSION");
    let nums: Vec<&str> = ver.split('.').chain(std::iter::repeat("0")).take(4).collect();
    let rc = format!(
        r#"1 ICON "{ico}"
1 VERSIONINFO
FILEVERSION {n}
PRODUCTVERSION {n}
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "Deadshot"
      VALUE "FileDescription", "Amboss Map-Editor"
      VALUE "FileVersion", "{ver}"
      VALUE "ProductName", "Amboss"
      VALUE "ProductVersion", "{ver}"
      VALUE "LegalCopyright", "(c) 2026 Deadshot, MIT OR Apache-2.0"
      VALUE "OriginalFilename", "amboss.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        ico = ico.display().to_string().replace('\\', "/"),
        n = nums.join(","),
    );
    let rc_path = out.join("amboss.rc");
    std::fs::write(&rc_path, rc).unwrap();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if env == "gnu" {
        let obj = out.join("amboss_res.o");
        for tool in ["x86_64-w64-mingw32-windres", "windres"] {
            if std::process::Command::new(tool).arg(&rc_path).arg("-O").arg("coff").arg("-o").arg(&obj).status().is_ok_and(|s| s.success()) {
                println!("cargo:rustc-link-arg-bins={}", obj.display());
                return;
            }
        }
    } else if env == "msvc" {
        let res = out.join("amboss.res");
        if std::process::Command::new("rc.exe").arg("/nologo").arg("/fo").arg(&res).arg(&rc_path).status().is_ok_and(|s| s.success()) {
            println!("cargo:rustc-link-arg-bins={}", res.display());
            return;
        }
    }
    println!("cargo:warning=Kein Ressourcen-Compiler gefunden – amboss.exe ohne Icon");
}

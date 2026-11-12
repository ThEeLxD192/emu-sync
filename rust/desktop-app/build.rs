//! Embeds the Windows icon, version information and manifest into the executable.

/// `longPathAware` lets saves live beneath paths longer than 260 characters. The executable
/// never needs elevation, and it declares Windows 10 and 11 as supported.
#[cfg(windows)]
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings xmlns:ws2="http://schemas.microsoft.com/SMI/2016/WindowsSettings">
      <ws2:longPathAware>true</ws2:longPathAware>
    </windowsSettings>
  </application>
</assembly>
"#;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../packaging/assets/icon.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows") {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("../../packaging/assets/icon.ico")
            .set_manifest(MANIFEST)
            .set("ProductName", "EmuSync")
            .set("FileDescription", "EmuSync")
            .set("InternalName", "EmuSync")
            .set("OriginalFilename", "EmuSync.exe");
        // A silent fallback would ship an executable without its icon or version.
        resource
            .compile()
            .expect("could not embed the Windows resources");
    }
}

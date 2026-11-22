// Windows builds: embeds the app icons (exe icon in Explorer + tray icons, 1 = normal,
// 2 = paused) and the version information shown in the exe's Properties.
fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");
    println!("cargo:rerun-if-changed=assets/app-paused.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let dir = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap()
        .replace('\\', "/");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let nums: Vec<&str> = version.split(['.', '-']).take(3).collect();
    let rc = format!(
        r#"1 ICON "{dir}/assets/app.ico"
2 ICON "{dir}/assets/app-paused.ico"
1 VERSIONINFO
FILEVERSION {n},0
PRODUCTVERSION {n},0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "ef tools"
      VALUE "FileDescription", "Activity Tracker"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "activity-tracker"
      VALUE "LegalCopyright", "Enzo Freminet"
      VALUE "OriginalFilename", "activity-tracker.exe"
      VALUE "ProductName", "Activity Tracker"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        n = nums.join(",")
    );
    let path = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("app.rc");
    std::fs::write(&path, rc).unwrap();
    embed_resource::compile(&path, embed_resource::NONE)
        .manifest_required()
        .unwrap();
}

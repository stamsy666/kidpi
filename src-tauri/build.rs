fn main() {
    // kidpi needs administrator rights: WinDivert (used by zapret's winws.exe)
    // can only be loaded by an elevated process.
    let windows = tauri_build::WindowsAttributes::new().app_manifest(include_str!("kidpi.exe.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");
}

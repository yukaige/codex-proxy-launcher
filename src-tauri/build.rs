fn main() {
    let windows_gnu = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu");
    let mut attributes = tauri_build::Attributes::new();
    if windows_gnu {
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    }
    tauri_build::try_build(attributes).expect("failed to run Tauri build script");

    // Embed the same manifest in the app and Cargo's unit-test executable.
    if windows_gnu {
        let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"))
            .join("test-manifest.o");
        let status = std::process::Command::new("windres")
            .args(["-i", "test-manifest.rc", "-o"])
            .arg(&output)
            .status()
            .expect("windres is required for Windows GNU builds");
        assert!(
            status.success(),
            "windres could not compile the test manifest"
        );
        println!("cargo:rustc-link-arg={}", output.display());
        println!("cargo:rerun-if-changed=test-manifest.rc");
        println!("cargo:rerun-if-changed=test-manifest.xml");
    }
}

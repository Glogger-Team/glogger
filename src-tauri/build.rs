fn main() {
    if is_windows_msvc() {
        embed_comctl_manifest_for_all_targets();
        tauri_build_without_bundled_manifest();
    } else {
        tauri_build::build();
    }
}

// Test-binary loader fix (0xc0000139): `cargo test` binaries died at the OS
// loader with 0xc0000139 (STATUS_ENTRYPOINT_NOT_FOUND; dumpbin pointed at
// comctl32 TaskDialogIndirect) while app binaries ran fine. Root cause is link
// scope: tauri-build embeds its Common-Controls v6 manifest via
// `embed_resource::compile`, which emits `cargo:rustc-link-arg-bins` (app
// binaries only), so the lib unit-test harness fell back to comctl32 v5.
// A test-only embed is NOT achievable with this toolchain: `PROFILE` is never
// `"test"`, and `cargo:rustc-link-arg-tests` hard-errors for packages with no
// `tests/` directory and — even with one — never reaches lib unit tests
// (rust-lang/cargo#10937; same disproof in persea-desktop#114). So the manifest
// rides the generic `cargo:rustc-link-arg`, which reaches every target of this
// package including the unit-test harness.
// Safety for the shipped binary: the manifest below is byte-equivalent to
// tauri-build 2.7.4's default `windows-app-manifest.xml`, and tauri's own RC
// manifest is disabled (`new_without_app_manifest`), so the linker input is the
// SINGLE RT_MANIFEST source — no double-embed, same Common-Controls v6 stanza
// the app ships today. Gated to Windows MSVC (`/MANIFESTINPUT` is link.exe
// syntax); every other platform/toolchain runs plain `tauri_build::build()`.
//
// Historic manual ritual (OBSOLETE - do not run): `cargo test --lib --no-run`,
// note the exe under `target/debug/build/glogger/<hash>/out/` (the
// `target/debug/deps/` glob misses it), write `<exe>.manifest` with a matching
// assemblyIdentity name plus the Common-Controls dependency, embed with
// `C:/Program Files (x86)/Windows Kits/10/bin/10.0.28000.0/x64/mt.exe
// -manifest <exe>.manifest -outputresource:<exe>;#1`, and re-embed after EVERY
// rebuild (cargo rewrites the exe). SDK pin seen on this box: 10.0.28000.0.
// This embed supersedes mt.exe (link-time, every build, no manual step).
const WINDOWS_APP_MANIFEST_XML: &str = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*" />
    </dependentAssembly>
  </dependency>
</assembly>
"#;

// Linker-argument form of the manifest gate: never `cfg!` in a build script
// (that checks the host); `CARGO_CFG_*` reflects the target being built.
fn is_windows_msvc() -> bool {
    matches!(
        std::env::var("CARGO_CFG_TARGET_OS").as_deref(),
        Ok("windows")
    ) && matches!(
        std::env::var("CARGO_CFG_TARGET_ENV").as_deref(),
        Ok("msvc")
    )
}

// Write the manifest to OUT_DIR and link it into every target via link.exe.
fn embed_comctl_manifest_for_all_targets() {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let manifest_path = out_dir.join("windows-app-manifest.xml");
    std::fs::write(&manifest_path, WINDOWS_APP_MANIFEST_XML).unwrap();
    println!("cargo::rustc-link-arg=/MANIFEST:EMBED");
    println!(
        "cargo::rustc-link-arg=/MANIFESTINPUT:{}",
        manifest_path.display()
    );
}

// `tauri_build::build()` minus the bundled app manifest. Failure handling is
// copied verbatim from `tauri_build::build` so diagnostics stay identical.
fn tauri_build_without_bundled_manifest() {
    if let Err(error) = tauri_build::try_build(
        tauri_build::Attributes::default().windows_attributes(
            tauri_build::WindowsAttributes::new_without_app_manifest(),
        ),
    ) {
        let error = format!("{error:#}");
        println!("{error}");
        if error.starts_with("unknown field") {
            print!(
                "found an unknown configuration field. This usually means that you are using a CLI version that is newer than `tauri-build` and is incompatible. "
            );
            println!(
                "Please try updating the Rust crates by running `cargo update` in the Tauri app folder."
            );
        }
        std::process::exit(1);
    }
}

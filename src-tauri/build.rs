use std::path::{Path, PathBuf};

fn main() {
    build_clock_dll();
    tauri_build::build()
}

/// Compiles `src-clock/clock.cpp` into `duri_clock.dll` in OUT_DIR, where
/// `taskbar_clock::inject` embeds it with `include_bytes!`. The `cc` crate
/// only finds MSVC and sets up its environment; the DLL is linked by `cl`.
fn build_clock_dll() {
    let source_dir = Path::new("../src-clock");
    let source = source_dir.join("clock.cpp");
    let def = source_dir.join("clock.def");
    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", def.display());

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let dll = out.join("duri_clock.dll");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        // Nothing to inject into elsewhere; keep include_bytes! happy.
        std::fs::write(&dll, []).expect("write placeholder dll");
        return;
    }

    let compiler = cc::Build::new().cpp(true).static_crt(true).warnings(false).get_compiler();
    let mut command = compiler.to_command();

    // C++/WinRT headers sit next to the SDK's `um` folder, which cc puts on
    // INCLUDE; cc itself doesn't add them.
    let include = compiler
        .get_envs()
        .rev()
        .find(|(key, _)| key.eq_ignore_ascii_case("INCLUDE"))
        .map(|(_, value)| value.to_os_string())
        .or_else(|| std::env::var_os("INCLUDE"))
        .unwrap_or_default();
    if let Some(um) = std::env::split_paths(&include).find(|path| path.ends_with("um")) {
        command.arg("/I").arg(um.with_file_name("cppwinrt"));
    }

    // Trailing separator: cl takes /Fo as a directory only then.
    let mut objects = out.clone().into_os_string();
    objects.push("\\");
    let mut fo = std::ffi::OsString::from("/Fo");
    fo.push(&objects);
    let mut fe = std::ffi::OsString::from("/Fe");
    fe.push(&dll);
    let mut def_arg = std::ffi::OsString::from("/DEF:");
    def_arg.push(&def);

    command
        .args(["/std:c++20", "/EHsc", "/O2", "/DUNICODE", "/D_UNICODE", "/DNDEBUG", "/LD"])
        .arg(&source)
        .arg(fo)
        .arg(fe)
        .arg("/link")
        .arg(def_arg)
        .args(["WindowsApp.lib", "user32.lib", "ole32.lib", "oleaut32.lib"]);

    let status = command.status().expect("failed to run the C++ compiler for src-clock/clock.cpp");
    assert!(status.success(), "building duri_clock.dll failed");
}

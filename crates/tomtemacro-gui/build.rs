fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon/tomtemacro.ico");
    embed_windows_icon();
}

// cfg(windows) in a build script is the HOST, so this covers native Windows
// builds (what CI and releases do) but not cross-compiling to Windows from
// another OS — there the exe just ships without an embedded icon.
#[cfg(windows)]
fn embed_windows_icon() {
    winresource::WindowsResource::new()
        .set_icon("../../assets/icon/tomtemacro.ico")
        .compile()
        .expect("failed to embed the Windows icon resource");
}

#[cfg(not(windows))]
fn embed_windows_icon() {}

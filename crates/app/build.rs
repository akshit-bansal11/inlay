//! Gives the exe its icon. `icon.res` is a compiled Windows resource written
//! ahead of time by `scripts/make_icon.py`, so the build needs no resource
//! compiler: the linker takes the file as it is. It also gives the exe its
//! version information, compiled from `version.rc` with Cargo.toml's version.

fn main() {
    println!("cargo:rerun-if-changed=icon.res");
    // Only the MSVC linker takes a .res file as an input.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let resource = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icon.res");
        println!("cargo:rustc-link-arg-bins={}", resource.display());
        // Version information from Cargo.toml's version, so Properties > Details and
        // security tools see what the exe is.
        println!("cargo:rerun-if-changed=version.rc");
        let version = env!("CARGO_PKG_VERSION");
        let mut parts = version.split(['.', '-']);
        let mut next = || parts.next().unwrap_or("0").to_owned();
        let (major, minor, patch) = (next(), next(), next());
        let macros = [
            format!("VER_MAJOR={major}"),
            format!("VER_MINOR={minor}"),
            format!("VER_PATCH={patch}"),
            format!("VER_STRING=\"{version}\""),
        ];
        embed_resource::compile("version.rc", &macros)
            .manifest_optional()
            .expect("version.rc compiles");
    }
}

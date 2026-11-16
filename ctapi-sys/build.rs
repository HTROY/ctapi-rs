use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir_string = env::var("OUT_DIR").unwrap();
    let manifest_dir_string = env::var("CARGO_MANIFEST_DIR").unwrap();
    let target = env::var("TARGET").unwrap();
    // Cargo has used both build/<hash>/out and build/<package>/<hash>/out.
    let target_profile = Path::new(&out_dir_string)
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "build"))
        .and_then(Path::parent)
        .expect("Cargo profile directory");
    let deps_dir = target_profile.join("deps");
    fs::create_dir_all(&deps_dir).expect("create Cargo dependencies directory");
    let mut lib_dir = Path::new(&manifest_dir_string).join("lib");

    if target.contains("i686") {
        lib_dir = lib_dir.join("x86");
    } else {
        lib_dir = lib_dir.join("x64");
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-link-search=native={}", &lib_dir.display());

    for entry in Path::new(&lib_dir)
        .read_dir()
        .expect("read dir call failed")
    {
        let entry = entry.unwrap();
        let path = entry.path();
        let destination = deps_dir.join(entry.file_name());
        if path.is_file() && !destination.exists() {
            fs::copy(&path, &destination).unwrap();
        }
    }
}

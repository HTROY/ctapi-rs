use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir_string = env::var("OUT_DIR").unwrap();
    let manifest_dir_string = env::var("CARGO_MANIFEST_DIR").unwrap();
    let target = env::var("TARGET").unwrap();
    let out_dir = Path::new(&out_dir_string)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();
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
        if path.is_file()
            && !Path::new(&out_dir)
                .join("deps")
                .join(path.file_name().unwrap())
                .as_path()
                .exists()
        {
            fs::copy(
                &path,
                Path::new(&out_dir)
                    .join("deps")
                    .join(path.file_name().unwrap()),
            )
            .unwrap();
        }
    }
}

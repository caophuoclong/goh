use std::{env, fs, path::PathBuf};

fn main() {
    let specs = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("specs");
    println!("cargo:rerun-if-changed=specs");
    let mut files: Vec<_> = fs::read_dir(&specs).expect("read bundled specs")
        .map(|entry| entry.expect("read spec entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    let mut generated = String::from("const BUNDLED: &[(&str, &str)] = &[\n");
    for file in files {
        let name = file.file_stem().and_then(|s| s.to_str()).expect("UTF-8 spec name");
        let path = file.to_str().expect("UTF-8 spec path");
        generated.push_str(&format!("    ({name:?}, include_str!({path:?})),\n"));
    }
    generated.push_str("];\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("bundled_specs.rs");
    fs::write(out, generated).expect("write bundled spec catalog");
}

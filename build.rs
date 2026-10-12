
use std::{ env::var_os, fs::{ read_dir, write }, path::{ Path, PathBuf } };

use chrono::Utc;


fn collect_modules(directory: &Path, files: &mut Vec<PathBuf>)
{
    for entry in read_dir(directory).expect("Cannot read the standard library")
    {
        let entry = entry.expect("Cannot read a standard-library entry");
        let path = entry.path();
        // Do not follow directory symlinks, which can lead back into the same tree.
        if entry.file_type().expect("Cannot read a standard-library file type").is_dir()
        { collect_modules(&path, files); }
        else if path.is_file() && path.extension().is_some_and(|extension| extension == "shy")
        { files.push(path); }
    }
}


fn main()
{
    let now = Utc::now();

    println!("cargo::rustc-env=SHELLY_BUILD_DATE={}", now.format("%Y-%m-%d"));
    println!("cargo::rustc-env=SHELLY_BUILD_TIME={}", now.format("%H:%M:%S UTC"));

    // Cargo installs the executable only. Carry the matching Shelly sources in it.
    let directory = PathBuf::from(var_os("CARGO_MANIFEST_DIR").unwrap()).join("std");
    println!("cargo::rerun-if-changed={}", directory.display());
    let mut files = Vec::new();
    collect_modules(&directory, &mut files);
    files.sort();
    let mut modules = String::from("const MODULES: &[(&str, &str)] = &[\n");
    for file in files
    {
        let relative = file.strip_prefix(&directory).unwrap().with_extension("");
        let name = relative.components().map(|component| component.as_os_str().to_str()
            .expect("Invalid standard-library module name")).collect::<Vec<_>>().join("/");
        modules.push_str(&format!("    ({:?}, include_str!({:?})),\n", name, file));
    }
    modules.push_str("];\n");
    let output = PathBuf::from(var_os("OUT_DIR").unwrap()).join("bundled_std.rs");
    write(output, modules).expect("Cannot generate the bundled standard library");
}

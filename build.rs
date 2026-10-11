
use std::{ env::var_os, fs::{ read_dir, write }, path::PathBuf };

use chrono::Utc;

fn main()
{
    let now = Utc::now();

    println!("cargo::rustc-env=SHELLY_BUILD_DATE={}", now.format("%Y-%m-%d"));
    println!("cargo::rustc-env=SHELLY_BUILD_TIME={}", now.format("%H:%M:%S UTC"));

    // Cargo installs the executable only. Carry the matching Shelly sources in it.
    let directory = PathBuf::from(var_os("CARGO_MANIFEST_DIR").unwrap()).join("std");
    println!("cargo::rerun-if-changed={}", directory.display());
    let mut files: Vec<_> = read_dir(&directory).expect("Cannot read the standard library")
        .map(|entry| entry.expect("Cannot read a standard-library entry").path())
        .filter(|path| path.is_file()
            && path.extension().is_some_and(|extension| extension == "shy"))
        .collect();
    files.sort();
    let mut modules = String::from("const MODULES: &[(&str, &str)] = &[\n");
    for file in files
    {
        let name = file.file_stem().unwrap().to_str()
            .expect("Invalid standard-library module name");
        modules.push_str(&format!("    ({:?}, include_str!({:?})),\n", name, file));
    }
    modules.push_str("];\n");
    let output = PathBuf::from(var_os("OUT_DIR").unwrap()).join("bundled_std.rs");
    write(output, modules).expect("Cannot generate the bundled standard library");
}

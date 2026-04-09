use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let manifest_path = PathBuf::from(&manifest_dir);
    let workspace_root = manifest_path.join("..").join("..");

    let profile = env::var("PROFILE").expect("PROFILE");
    let target_resources = workspace_root
        .join("target")
        .join(&profile)
        .join("resources");
    let src_resources = workspace_root.join("resources");

    if src_resources.is_dir() {
        println!("cargo:rerun-if-changed={}", src_resources.display());
        copy_dir_merge(&src_resources, &target_resources)
            .unwrap_or_else(|e| panic!("failed to copy {} -> {}: {}", src_resources.display(), target_resources.display(), e));
    }

    let lib_path = manifest_path.join("..\\..\\lib\\windows\\amd64");
    println!("cargo:rustc-link-search=native={}", lib_path.display());
}

/// Copy tree from `src` into `dst`, creating `dst` and overwriting files with same names.
fn copy_dir_merge(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let dest_path = dst.join(&name);
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_merge(&path, &dest_path)?;
        } else {
            fs::copy(&path, &dest_path)?;
        }
    }
    Ok(())
}

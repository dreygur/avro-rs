fn main() {
    let fcitx_core = pkg_config::Config::new()
        .probe("Fcitx5Core")
        .expect("Fcitx5Core not found — install fcitx5-devel");
    let fcitx_utils = pkg_config::Config::new()
        .probe("Fcitx5Utils")
        .expect("Fcitx5Utils not found");

    // Fcitx5 headers use std::span and string_view::starts_with (C++20).
    let mut build = cc::Build::new();
    build.cpp(true).std("c++20").file("src/shim.cpp");

    // Allow caller to override install data dir via env (used by Makefile).
    let pkgdatadir =
        std::env::var("PKGDATADIR").unwrap_or_else(|_| "/usr/share/fcitx5/avro".to_string());
    build.define("PKGDATADIR", Some(format!("\"{pkgdatadir}\"").as_str()));

    for path in fcitx_core
        .include_paths
        .iter()
        .chain(fcitx_utils.include_paths.iter())
    {
        build.include(path);
    }

    build.compile("avro_shim");

    // Re-link Fcitx5 shared libs so the cdylib resolves them.
    for lib in fcitx_core.libs.iter().chain(fcitx_utils.libs.iter()) {
        println!("cargo:rustc-link-lib={lib}");
    }
    for path in fcitx_core
        .link_paths
        .iter()
        .chain(fcitx_utils.link_paths.iter())
    {
        println!("cargo:rustc-link-search={}", path.display());
    }

    println!("cargo:rerun-if-changed=src/shim.cpp");
}

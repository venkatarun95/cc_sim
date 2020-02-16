extern crate bindgen;
extern crate cc;

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/bbr/simul.c");

    cc::Build::new()
		.file("src/bbr/simul.c")
		.compile("simul");

    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("src/bbr/linux_bbr/ns-linux-util.h")
        .header("src/bbr/map.h")
        .header("src/bbr/simul.h")
        .trust_clang_mangling(false)
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bbr.rs"))
        .expect("Couldn't write bindings!");
    
    bindings
        .write_to_file("src/bbr/bbr.rs")
        .expect("Couldn't write bindings!");
}
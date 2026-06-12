fn main() {
    // Compile a small C shim that wraps libdispatch macros/symbols.
    // dispatch_get_main_queue() is a macro on macOS, so we can't call it via extern "C" directly.
    cc::Build::new()
        .file("src/dispatch_shim.c")
        .compile("ah_dispatch_shim");

    // Link against the system frameworks
    println!("cargo:rustc-link-lib=framework=Foundation");
}

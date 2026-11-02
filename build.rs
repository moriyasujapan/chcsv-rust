use std::path::Path;

// Compile the vendored ODPI-C sources statically into the binary.
// ODPI-C loads the Oracle client library (libclntsh.so) at runtime via dlopen,
// so no Oracle libraries are required at link time.
fn main() {
    println!("cargo:rerun-if-changed=vendor/odpi/src");
    println!("cargo:rerun-if-changed=vendor/odpi/include");

    let odpi = Path::new("vendor/odpi");
    if !odpi.join("include/dpi.h").exists() {
        panic!("vendor/odpi not found. Run ./setup.sh first to fetch ODPI-C sources.");
    }

    // Publish the ODPI version macros so the FFI can request the right version.
    let header = std::fs::read_to_string(odpi.join("include/dpi.h")).unwrap();
    let major = find_define(&header, "DPI_MAJOR_VERSION");
    let minor = find_define(&header, "DPI_MINOR_VERSION");
    println!("cargo:rustc-env=DPI_MAJOR_VERSION={major}");
    println!("cargo:rustc-env=DPI_MINOR_VERSION={minor}");

    let mut build = cc::Build::new();
    build
        .include(odpi.join("include"))
        .include(odpi.join("src"))
        .flag_if_supported("-std=c99")
        .flag_if_supported("-fvisibility=hidden")
        .warnings(false);

    let src = odpi.join("src");
    let mut files: Vec<_> = std::fs::read_dir(&src)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|x| x == "c").unwrap_or(false))
        .collect();
    files.sort();
    for f in &files {
        build.file(f);
    }
    build.compile("libodpi.a");

    // ODPI uses dlopen()/dlsym() on Linux.
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "linux" {
        println!("cargo:rustc-link-lib=dl");
        println!("cargo:rustc-link-lib=pthread");
        // Make the binary find a bundled ./instantclient_* directory with zero
        // environment setup (dlopen from this executable honors its RPATH).
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/instantclient_21_21");
        println!("cargo:rustc-link-arg=-Wl,--enable-new-dtags");
    }
}

fn find_define(header: &str, name: &str) -> String {
    for line in header.lines() {
        let line = line.trim();
        if line.starts_with(&format!("#define {name}")) {
            if let Some(v) = line.split_whitespace().nth(2) {
                return v.to_string();
            }
        }
    }
    panic!("could not find #define {name} in vendor/odpi/include/dpi.h");
}

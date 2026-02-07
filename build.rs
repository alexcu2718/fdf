#![allow(clippy::undocumented_unsafe_blocks)]

const MIN_THREADS: usize = 1;

#[cfg(target_os = "linux")]
fn get_supported_filesystems() -> Result<Vec<String>, std::io::Error> {
    use std::io::BufRead as _;
    let file = std::fs::File::open("/proc/filesystems")?;
    let reader = std::io::BufReader::new(file);
    let mut filesystems: Vec<String> = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if let Some(fs_name) = parts.last() {
            filesystems.push((*fs_name).to_owned());
        }
    }

    Ok(filesystems)
}

#[allow(clippy::unwrap_used)]
fn check_dirent_has_field(cfg_name: &str) {
    // Tell cargo about the cfg we intend to use so `check-cfg` won't warn.
    println!("cargo:rustc-check-cfg=cfg({cfg_name})");
    let out = std::env::var("OUT_DIR").unwrap();

    let c_file = format!("check_{cfg_name}.c");

    let src = std::path::PathBuf::from(&out).join(&c_file);

    // This C source fails to compile if the struct field is not present.
    // We derive the field name from the `cfg_name`, which is of the form `has_<field>`.
    let field_name = cfg_name.strip_prefix("has_").unwrap_or(cfg_name).to_owned();
    assert!(
        field_name.starts_with("d_"),
        "Field name must start with d_"
    );

    let code = format!(
        // use stddef.h to get offsetof
        "#include <dirent.h>\n#include <stddef.h>\nstatic const size_t off = offsetof(struct dirent, {field_name});\nint main(void) {{ (void)off; return 0; }}\n",
    );
    std::fs::write(&src, code).unwrap();

    let mut build = cc::Build::new();
    build.file(&src).cargo_warnings(false).cargo_output(true);

    if build.try_compile(&c_file).is_ok() {
        // Enable the flag
        println!("cargo:rustc-cfg={cfg_name}")
    }
}

fn main() {
    // Re-run build script if filesystem list changes
    #[cfg(target_os = "linux")]
    println!("cargo:rerun-if-changed=/proc/filesystems");

    //set threadcounts for rayon.
    let num_threads =
        std::thread::available_parallelism().map_or(MIN_THREADS, core::num::NonZeroUsize::get);

    println!("cargo:rustc-env=THREAD_COUNT={num_threads}");

    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    println!("cargo:rustc-env=FDF_PAGE_SIZE={page_size}");

    // Check for reiser and stop building if so
    #[cfg(target_os = "linux")]
    match get_supported_filesystems() {
        Ok(filesystems) => {
            let has_reiser = filesystems.iter().any(|fs| fs.starts_with("reiser"));
            // Crash on reiser support
            assert!(!has_reiser, "reiser file systems not supported");
        }
        Err(e) => {
            println!("cargo:warning=Failed to read /proc/filesystems: {e}");
        }
    }

    check_dirent_has_field("has_d_type");

    check_dirent_has_field("has_d_reclen");

    check_dirent_has_field("has_d_namlen");

    check_dirent_has_field("has_d_ino");
}

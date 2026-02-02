#![allow(clippy::undocumented_unsafe_blocks)]

const MIN_THREADS: usize = 1;

#[cfg(target_os = "macos")]
unsafe fn getdirentries64(
    fd: libc::c_int,
    buffer_ptr: *mut i8,
    nbytes: libc::size_t,
    basep: *mut libc::off_t,
) -> i32 {
    const SYS_GETDIRENTRIES64: libc::c_int = 344; // Reverse engineered syscall number
    //https://phrack.org/issues/66/16
    unsafe { libc::syscall(SYS_GETDIRENTRIES64, fd, buffer_ptr, nbytes, basep) }
}

#[cfg(target_os = "macos")]
#[allow(clippy::expect_used)]
#[allow(clippy::multiple_unsafe_ops_per_block)]
// Because we're rawdogging an undocumented syscall, we need to check if it doesnt change
// So this SHOULD always return >=0 from the syscall
fn test_macos_syscall() {
    use std::env::temp_dir;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt as _;
    let tmp = temp_dir();
    let test_dir = CString::new(tmp.as_os_str().as_bytes()).expect("CString conversion failed");

    unsafe {
        let dir_fd = libc::open(test_dir.as_ptr(), libc::O_RDONLY);
        assert!(dir_fd >= 0, "Failed to open directory for testing");

        let mut buffer: [i8; 1024] = [0; 1024];
        let mut basep: libc::off_t = 0;

        let result = getdirentries64(dir_fd, buffer.as_mut_ptr(), buffer.len(), &raw mut basep);

        libc::close(dir_fd);

        // Note: A result of 0 means no more entries, which is valid
        // Negative values indicate errors
        assert!(
            result >= 0,
            "getdirentries64 syscall test failed with result: {result}\n This indicates the syscall number has changed!",
        )
    }
}

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
fn check_dirent_has_field(macro_name: &str, cfg_name: &str) {
    // Tell cargo about the cfg we intend to use so `check-cfg` won't warn.
    println!("cargo:rustc-check-cfg=cfg({cfg_name})");
    let out = std::env::var("OUT_DIR").unwrap();

    let c_file = format!("check_{macro_name}.c");

    let src = std::path::PathBuf::from(&out).join(&c_file);

    // This C source fails to compile if the struct field is not present.
    // We derive the field name from the `cfg_name`, which is of the form `has_<field>`.
    let field_name = cfg_name.strip_prefix("has_").unwrap_or(cfg_name).to_owned();

    let code = format!(
        // use stddef.h to get offsetof
        "#include <dirent.h>\n#include <stddef.h>\nstatic const size_t off = offsetof(struct dirent, {field_name});\nint main(void) {{ (void)off; return 0; }}\n",
    );
    std::fs::write(&src, code).unwrap();

    let mut build = cc::Build::new();
    build.file(&src).cargo_warnings(true).cargo_output(true);

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

    #[cfg(target_os = "macos")]
    test_macos_syscall();

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

    check_dirent_has_field("_DIRENT_HAVE_D_TYPE", "has_d_type");

    check_dirent_has_field("_DIRENT_HAVE_D_RECLEN", "has_d_reclen");

    check_dirent_has_field("_DIRENT_HAVE_D_NAMLEN", "has_d_namlen");

    check_dirent_has_field("_DIRENT_HAVE_D_INO", "has_d_ino");
}

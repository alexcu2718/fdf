#![allow(clippy::undocumented_unsafe_blocks)]

// Create a purposefully empty directory and test if the EOF trick is used by the kernel
#[cfg(target_os = "macos")]
#[allow(clippy::expect_used, clippy::let_underscore_must_use)]
fn test_eof() {
    use std::os::unix::ffi::OsStrExt as _;
    /*
    https://github.com/apple-oss-distributions/Libc/blob/899a3b2d52d95d75e05fb286a5e64975ec3de757/gen/FreeBSD/opendir.c#L373-L392
    #if __DARWIN_64_BIT_INO_T
            /*
             * sufficiently recent kernels when the buffer is large enough,
             * will use the last bytes of the buffer to return status.
             *
             * To support older kernels:
             * - make sure it's 0 initialized
             * - make sure it's past `dd_size` before reading it
             */
            getdirentries64_flags_t *gdeflags =
                (getdirentries64_flags_t *)(dirp->dd_buf + dirp->dd_len -
                sizeof(getdirentries64_flags_t));
            *gdeflags = 0;
            dirp->dd_size = (long)__getdirentries64(dirp->dd_fd,
                dirp->dd_buf, dirp->dd_len, &dirp->dd_td->seekoff);
            if (dirp->dd_size >= 0 &&
                dirp->dd_size <= dirp->dd_len - sizeof(getdirentries64_flags_t)) {
                if (*gdeflags & GETDIRENTRIES64_EOF) {
                    dirp->dd_flags |= __DTF_ATEND;
                }
            }

     */

    // Tell cargo about the cfg we intend to use so `check-cfg` won't warn.
    println!("cargo:rustc-check-cfg=cfg(has_eof_trick)");

    // link to libc (as done it in main crate)
    unsafe extern "C" {
        fn __getdirentries64(
            fd: libc::c_int,
            buf: *mut libc::c_char,
            nbytes: libc::size_t,
            basep: *mut libc::off_t,
        ) -> libc::ssize_t;
    } // Compile error if this doesn't link.

    const BUFFER_SIZE: usize = 4096;

    let tmp = std::env::temp_dir();

    let empty = tmp.join("MACOSEOFTESTINGDIR");
    std::fs::create_dir_all(&empty).expect("MACOS empty dir not created!");
    // Guaranteed null terminated
    let empty_cstring = std::ffi::CString::new(empty.as_os_str().as_bytes())
        .expect("temporary dir Cstring not created!");

    const FLAGS: i32 = libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NONBLOCK;
    // SAFETY: guaranteed null terminated
    let get_fd = unsafe { libc::open(empty_cstring.as_ptr(), FLAGS) };

    assert!(get_fd > 0, "Unexpected error in opening temporary fd!");
    let mut buffer = [0u8; BUFFER_SIZE];
    let mut base_pointer = 0i64;
    // SAFETY: valid buffer+size+pointer
    unsafe {
        __getdirentries64(
            get_fd,
            buffer.as_mut_ptr().cast(),
            BUFFER_SIZE,
            &raw mut base_pointer,
        )
    };

    let last_four_bytes = &buffer[BUFFER_SIZE - 4..];
    // We don't need to care about endianness since macos is LE only
    //(well, tbf, I don't care for even googling if some old ass macos from 2002 on a big endian chinese esoteric CPU(on a thinkpad no less...) is BE
    let has_eof = last_four_bytes == [1, 0, 0, 0];
    // If the last 4 bytes are in this arrangement, we know that the kernel is using the EOF trick.

    if has_eof {
        println!("cargo:rustc-cfg=has_eof_trick")
    }

    let _ = std::fs::remove_dir_all(&empty);
}

#[cfg(target_os = "linux")]
fn get_supported_filesystems() {
    use std::io::BufRead as _;
    #[cfg(target_os = "linux")]
    println!("cargo:rerun-if-changed=/proc/filesystems");
    let Ok(file) = std::fs::File::open("/proc/filesystems") else {
        println!("cargo:warning=Failed to read /proc/filesystems");
        return;
    };
    let reader = std::io::BufReader::new(file);
    let mut filesystems: Vec<String> = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if let Some(fs_name) = parts.last() {
            filesystems.push((*fs_name).to_owned());
        }
    }

    let has_reiser = filesystems.iter().any(|fs| fs.starts_with("reiser"));
    // Crash on reiser support
    assert!(!has_reiser, "reiser file systems not supported");
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

    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    println!("cargo:rustc-env=FDF_PAGE_SIZE={page_size}");

    // Check for reiser and stop building if so
    #[cfg(target_os = "linux")]
    get_supported_filesystems();

    #[cfg(target_os = "macos")]
    test_eof();

    check_dirent_has_field("has_d_type");

    check_dirent_has_field("has_d_reclen");

    check_dirent_has_field("has_d_namlen");

    check_dirent_has_field("has_d_ino");
}

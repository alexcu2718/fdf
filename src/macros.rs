#[macro_export]
/**
 A helper macro to safely access dirent(64 on linux)'s
  fields of a `libc::dirent`/`libc::dirent64` aka 'dirent-type' struct by offset.

  # Safety
  - The caller must ensure that the pointer is valid and points to a 'dirent-type' struct.
  - The field name must be a valid field of the 'dirent-type' struct.

  # Field Aliases
  - On BSD systems (FreeBSD, OpenBSD, etc ), `d_ino` is aliased to `d_fileno`
  - On Linux, `d_reclen` is used to access the record length directly,
  - On MacOS/BSD, `d_namlen` is used to access the name length directly,
  # Usage
  ```ignore
  let entry_ptr: *const libc::dirent = ...; // Assume this is a valid pointer to a dirent struct
  let d_name_ptr:*const _ = access_dirent!(entry_ptr, d_name);
  let d_reclen:usize = access_dirent!(entry_ptr, d_reclen);

  let d_namlen:usize = access_dirent!(entry_ptr, d_namlen); // This is a special case for BSD and MacOS, where d_namlen is available
  let d_ino :u64= access_dirent!(entry_ptr, d_ino); // This
  ```
*/
macro_rules! access_dirent {
    // Special case for `d_reclen`
    ($entry_ptr:expr, d_reclen) => {{
        // SAFETY: Caller must ensure pointer is valid
        (*$entry_ptr).d_reclen as usize // /return usize
    }};
     // Special case for `d_namlen`


    ($entry_ptr:expr, d_namlen) => {{
        // SAFETY: Caller must ensure pointer is valid
        (*$entry_ptr).d_namlen as usize
    }};


    ($entry_ptr:expr, d_off) => {{
        // SAFETY: Caller must ensure pointer is valid
        (*$entry_ptr).d_off
    }};
    ($entry_ptr:expr,d_name) => {{
        //see reference https://github.com/rust-lang/rust/blob/8712e4567551a2714efa66dac204ec7137bc5605/library/std/src/sys/fs/unix.rs#L740
        //explanation also copypasted below.
         (&raw const (*$entry_ptr).d_name).cast::<u8>() //we have to have treat  pointer  differently because it's not guaranteed to actually be [0,256] (can't be worked with by value!)
    }};

         ($entry_ptr:expr, d_type) => {{
        #[cfg(any(
        target_os = "solaris",
        target_os = "illumos",
        target_os = "aix",
        target_os = "nto",
        target_os = "vita",
    ))]
        {
            libc::DT_UNKNOWN //return D_TYPE unknown on these OS'es, because the struct does not hold the type!
            //https://github.com/rust-lang/rust/blob/d85276b256a8ab18e03b6394b4f7a7b246176db7/library/std/src/sys/fs/unix.rs#L314
        }
        #[cfg(not(any(
        target_os = "solaris",
        target_os = "illumos",
        target_os = "aix",
        target_os = "nto",
        target_os = "vita", // via is still broken because it does not possess ino, i will consider an ino fix maybe but it's....extremely niche.
    )))]
        {
            (*$entry_ptr).d_type
        }}};
      // Handle inode number field with aliasing for BSD systems
    ($entry_ptr:expr, d_ino) => {{
        #[cfg(any(
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        ))]
        {
            // SAFETY: Caller must ensure pointer is valid
             (*$entry_ptr).d_fileno as u64
        }

        #[cfg(not(any(
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        )))]
        {
            // SAFETY: Caller must ensure pointer is valid
             (*$entry_ptr).d_ino
        }
    }};


}

/*

   // The dirent64 struct is a weird imaginary thing that isn't ever supposed
                // to be worked with by value. Its trailing d_name field is declared
                // variously as [c_char; 256] or [c_char; 1] on different systems but
                // either way that size is meaningless; only the offset of d_name is
                // meaningful. The dirent64 pointers that libc returns from readdir64 are
                // allowed to point to allocations smaller _or_ LARGER than implied by the
                // definition of the struct.
                //
                // As such, we need to be even more careful with dirent64 than if its
                // contents were "simply" partially initialized data.
                //
                // Like for uninitialized contents, converting entry_ptr to `&dirent64`
                // would not be legal. However, we can use `&raw const (*entry_ptr).d_name`
                // to refer the fields individually, because that operation is equivalent
                // to `byte_offset` and thus does not require the full extent of `*entry_ptr`
                // to be in bounds of the same allocation, only the offset of the field
                // being referenced.

                // d_name is guaranteed to be null-terminated.
                let name = CStr::from_ptr((&raw const (*entry_ptr).d_name).cast());
                let name_bytes = name.to_bytes();
                if name_bytes == b"." || name_bytes == b".." {
                    continue;
                }
*/

///A macro to safely access stat entries in a filesystem independent way
// TODO! add other fields as appropriate (this could be pretty long)
// will be public when kinks are worked out
macro_rules! access_stat {
    ($stat_struct:expr, st_mtimensec) => {{
        #[cfg(target_os = "netbsd")]
        {
            $stat_struct.st_mtimensec as _
        } //why did they do such a specific change

        #[cfg(not(target_os = "netbsd"))]
        {
            $stat_struct.st_mtime_nsec as _
        }
    }};

    ($stat_struct:expr, st_mtime) => {{ $stat_struct.st_mtime as _ }};

    // inode number, normalised to u64 for compatibility
    ($stat_struct:expr, st_ino) => {{
        #[cfg(any(
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        ))]
        {
            $stat_struct.st_ino as u64
        }

        #[cfg(not(any(
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        )))]
        {
            $stat_struct.st_ino
        }
    }};

    // Fallback for other fields
    ($stat_struct:expr, $field:ident) => {{ $stat_struct.$field as _ }};
}

#[macro_export]
/// A compile time assert, mirroring `static_assert` from C++
macro_rules! const_assert {
    ($cond:expr $(,)?) => {
        const _: () = {
            if !$cond {
                panic!(concat!("const assertion failed: ", stringify!($cond)));
            }
        };
    };
    ($cond:expr, $($arg:tt)+) => {
        const _: () = {
            if !$cond {
                panic!($($arg)+);
            }
        };
    };
}

#[cfg(any(
    target_os = "linux",
    target_os = "solaris",
    target_os = "illumos",
    target_os = "android"
))]
pub const MINIMUM_DIRENT_SIZE: usize =
    core::mem::offset_of!(crate::dirent64, d_name).next_multiple_of(8); //==24 for the platforms we care about
// Finding the minimum struct size, which is basically all the fields minus the variable part

#[cfg(any(
    target_os = "linux",
    target_os = "solaris",
    target_os = "illumos",
    target_os = "android"
))]
const_assert!(
    MINIMUM_DIRENT_SIZE == 24,
    "the minimum struct size isn't 24! BIG ERROR"
);

/**
 An optimised macro for skipping "." and ".." directory entries

 This macro employs several heuristics to efficiently skip the common "." and ".."
 entries present in every directory. The approach reduces unnecessary work during
 directory traversal and improves CPU branch prediction behaviour.

 Optimisation Strategy:
 1. TYPE FILTERING:
    Since "." and ".." are always directories (or occasionally unknown on unusual
    filesystems), the macro checks `d_type` or `d_namlen` first. This takes advantage
    of the fact that only about ten percent of filesystem entries are directories.
    Consequently:
    - The branch is easier for the CPU to predict
    - Expensive string length or comparison operations are avoided for roughly ninety percent of entries

 2. PLATFORM-SPECIFIC OPTIMISATIONS:
    - Linux, Solaris, Illumos: Uses the known property that `d_reclen` equals
      `OFFSET_OF_NAME(24)` for the "." and ".." entries.
    - BSD systems (macOS, FreeBSD, OpenBSD, NetBSD): Uses `d_namlen` for quick
      length checks.
    - Other systems: Falls back to a safe byte-by-byte comparison.

 Why this matters:
 - These checks are performed for every directory entry during traversal.
 - Standard traversal code often relies on `strcmp` or `strlen`; this approach
   avoids those calls where possible.
 - Improved branch prediction provides cumulative performance benefits
   across large directory trees.
*/
macro_rules! skip_dot_or_dot_dot_entries {
    ($entry:expr, $action:expr) => {{
        #[allow(unused_unsafe)]
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        /*
        SAFETY: when calling this macro, the pointer has already been ensured to be non-null
        This is internal only because it relies on internal heuristics/guarantees
        */
        unsafe {
            #[cfg(any(
                target_os = "macos",
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "netbsd"
            ))]
            {
                // BSD/macOS optimisation: check d_namlen first as primary filter
                let namelen = access_dirent!($entry, d_namlen);
                if namelen <= 2 {
                    // Only check d_type for potential "." or ".." entries
                    match access_dirent!($entry, d_type) {
                        libc::DT_DIR | libc::DT_UNKNOWN => {
                            let name_ptr = access_dirent!($entry, d_name);
                            // Combined check using pattern
                            match (namelen, *name_ptr.add(0), *name_ptr.add(1)) {
                                (1, b'.', _) => $action,    // "." - length 1, first char '.'
                                (2, b'.', b'.') => $action, // ".." - length 2, both chars '.'
                                _ => (),
                            }
                        }
                        _ => (),
                    }
                }
                // If namelen > 2, skip all checks entirely (covers ~99% of entries)
            }

            #[cfg(any(
                target_os = "linux",
                target_os = "solaris",
                target_os = "illumos",
                target_os = "android"
            ))]
            {
                // Linux/Solaris/Illumos optimisation: check d_type first
                match access_dirent!($entry, d_type) {
                    libc::DT_DIR | libc::DT_UNKNOWN => {
                        if access_dirent!($entry, d_reclen) == $crate::macros::MINIMUM_DIRENT_SIZE {
                            let name_ptr = access_dirent!($entry, d_name);
                            match (*name_ptr.add(0), *name_ptr.add(1), *name_ptr.add(2)) {
                                (b'.', 0, _) | (b'.', b'.', 0) => $action, //similar to above
                                _ => (),
                            }
                        }
                    }
                    _ => (),
                }
            }

            #[cfg(not(any(
                target_os = "linux",
                target_os = "macos",
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "netbsd",
                target_os = "solaris",
                target_os = "illumos",
                target_os = "android"
            )))]
            {
                // Fallback for other systems: check d_type first
                match access_dirent!($entry, d_type) {
                    libc::DT_DIR | libc::DT_UNKNOWN => {
                        let name_ptr = access_dirent!($entry, d_name);
                        match (*name_ptr.add(0), *name_ptr.add(1), *name_ptr.add(2)) {
                            (b'.', 0, _) | (b'.', b'.', 0) => $action,
                            _ => (),
                        }
                    }
                    _ => (),
                }
            }
        }
    }};
}

/// Macro to create a const from an env var with compile-time parsing (Please read the docs carefully)
///
///
/// const_from_env!(LOCAL_PATH_MAX: usize = "LOCAL_PATH_MAX", "X");, where X(usize) is the default value if the env var is not set
///
/// Example usage:
/// ```
/// use fdf::const_from_env;
/// const_from_env!(MYVAR: usize = "NYVAR", 6969);
/// assert_eq!(MYVAR, 6969); //6969 is the default value if the environment variable NYVAR is not set
/// ```
/// /// This macro allows you to define a constant that can be set via an environment variable at compile time.`
/// I realise people could have massive filesystems, i should probably write a rebuild script on value change.TODO!
/// Macro to create a const from an env var with compile-time parsing
///
/// # Usage
/// ```
/// use fdf::const_from_env;
///
/// const_from_env!(
///     /// Maximum path length for local filesystem operations
///     /// Default: 4096 (typical Linux PATH_MAX)
///     LOCAL_PATH_MAX: usize = "`LOCAL_PATH_MAX`", 4096
/// );
///
/// assert_eq!(LOCAL_PATH_MAX, 4096);
/// ```
///
/// # Notes
/// - The value is parsed at compile time
/// - Environment variables must contain only numeric characters
/// - Consider rebuilding if environment variable changes (TODO: add rebuild script)
#[macro_export]
#[allow(clippy::doc_markdown)]
macro_rules! const_from_env {
    ($(#[$meta:meta])* $name:ident: $t:ty = $env:expr, $default:expr) => {
        $(#[$meta])*
        pub const $name: $t = {
            // A helper const function to parse a string into a number.
            // This is used only when an environment variable is found.
            #[allow(clippy::single_call_fn)]
            #[allow(clippy::indexing_slicing)] //this will panic at compile time, intentionally.
            const fn parse_env(s: &str) -> $t {
                let mut n: $t = 0;
                let s_bytes = s.as_bytes();
                let mut i = 0;

                while i < s_bytes.len() {
                    let b = s_bytes[i];
                    match b {
                        b'0'..=b'9' => {
                            n = n * 10 + (b - b'0') as $t;
                        }
                        _ => panic!(concat!("Invalid numeric value in environment variable: ", stringify!($env))),
                    }
                    i += 1;
                }
                n
            }

            // Check if the environment variable is set.
            match option_env!($env) {
                // If it's set, parse the string value.
                Some(val) => parse_env(val),
                // If not, use the default
                None => $default as _,
            }
        };
    };
}

/// An internal macro to handle depth limits, we do it in a macro to avoid cloning calls and to make the code a lot cleaner!
macro_rules! handle_depth_limit {
    ($self:expr, $dir:expr, $should_send:expr, $sender:expr) => {
        #[allow(clippy::cast_possible_truncation, reason = "depth wont exceed u32")]
        if $self
            .search_config
            .depth
            .is_some_and(|depth| $dir.depth >= depth.get() as _)
        {
            if $should_send {
                if let Err(e) = $sender.send(vec![$dir]) {
                    if $self.search_config.show_errors {
                        eprintln!("Error sending DirEntry: {e}");
                    }
                }
            }
            return; // stop processing this directory if depth limit is reached
        }
    };
}

/// Extremely simple macro for getting rid of boiler blates
macro_rules! return_os_error {
    () => {{
        return Err(std::io::Error::last_os_error().into());
    }};
}

/// Macro for safely calling stat-like functions and handling the result, I might make it public?
macro_rules! stat_syscall {
    // For fstatat with flags
    ($syscall:ident, $fd:expr, $path:expr, $flags:expr) => {{
        let mut stat_buf = core::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY:
        // - The path is guaranteed to be null-terminated (CStr)
        let res = unsafe {
            $syscall(
                $fd.0,
                $path.as_ptr(),
                stat_buf.as_mut_ptr(),
                $flags,
            )
        };

        if res == 0 {
            // SAFETY: If the return code is 0, we know the stat structure has been properly initialized
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(std::io::Error::last_os_error().into())
        }
    }};
      // For fstatat with flags - returns FileType directly (kinda like an internal black magic tool for me to save writing so much duplicate code)
     ($syscall:ident, $fd:expr, $path:expr, $flags:expr,DTYPE) => {{
        let mut stat_buf = core::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY:
        // - The path is guaranteed to be null-terminated (CStr)
        let res = unsafe {
            $syscall(
                $fd.0,
                $path.as_ptr(),
                stat_buf.as_mut_ptr(),
                $flags,
            )
        };

        if res == 0 {
            // SAFETY: If the return code is 0, we know it's been initialised properly
            $crate::FileType::from_stat(&unsafe { stat_buf.assume_init() })
        } else {
            $crate::FileType::Unknown
        }
    }};



    // For stat/lstat with path pointer
    ($syscall:ident, $path_ptr:expr) => {{
        let mut stat_buf = core::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: We know the path is valid because internally it's a cstr
        let res = unsafe { $syscall($path_ptr, stat_buf.as_mut_ptr()) };

        if res == 0 {
            // SAFETY: If the return code is 0, we know it's been initialised properly
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(std::io::Error::last_os_error().into())
        }
    }};


}

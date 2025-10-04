#![allow(unused_macros)]

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
        #[cfg(any(target_os = "solaris", target_os = "illumos"))]
        {
            libc::DT_UNKNOWN //return D_TYPE unknown on these OS'es, because the struct does not hold the type!
        }
        #[cfg(not(any(target_os = "solaris", target_os = "illumos")))]
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
/// Creates a null-terminated C-style string pointer from a byte slice without allocation.
///
/// This macro provides two variants for creating stack-allocated C-strings:
/// 1. Default variant using `LOCAL_PATH_MAX` buffer size
/// 2. Custom variant allowing specification of buffer size
///
/// Both variants return a pointer to a null-terminated string in a properly aligned buffer.
/// The pointer type (`*const i8` or `*const u8`) is inferred by the calling context.
///
/// # Safety
///
/// The returned pointer is only valid for the current scope. The underlying buffer
/// will be destroyed when the current block exits. Using the pointer after the
/// current statement is undefined behavior.
///
/// # Panics
///
/// In debug builds, panics if the input byte slice length exceeds the buffer capacity.
///
/// # Variants
///
/// ## Default Variant (`cstr!(bytes)`)
///
/// Uses the default buffer size defined by `LOCAL_PATH_MAX`.
///
/// ## Custom Variant (`cstr!(bytes, size)`)
///
/// Allows specifying a custom buffer size as a compile-time constant.
/// This avoids potential issue of stack clashes!
///
/// # Examples
///
/// ## Default variant examples
///
/// Basic usage with type inference:
/// ```
/// # use fdf::cstr;
///
/// let s:*const u8 = unsafe { cstr!(b"hello") };
/// ```
///
/// Working with different pointer types:
/// ```
/// # use fdf::cstr;
/// let as_i8 = unsafe { cstr!(b"hello") } as *const i8;
/// let as_u8 = unsafe { cstr!(b"world") } as *const u8;
/// unsafe {
///     assert_eq!(*as_i8.offset(0), b'h' as i8);
///     assert_eq!(*as_u8.offset(0), b'w' as u8);
/// }
/// ```
///
/// Proper null termination:
/// ```
/// # use fdf::cstr;
/// let s: *const u8 = unsafe { cstr!(b"test") };
/// unsafe {
///     assert_eq!(*s.offset(0), b't');
///     assert_eq!(*s.offset(1), b'e');
///     assert_eq!(*s.offset(2), b's');
///     assert_eq!(*s.offset(3), b't');
///     assert_eq!(*s.offset(4), 0);
/// }
/// ```
///
/// Empty string case:
/// ```
/// # use fdf::cstr;
/// let empty: *const u8 = unsafe { cstr!(b"") };
/// unsafe {
///     assert_eq!(*empty.offset(0), 0);
/// }
/// ```
///
/// ## Custom variant examples
///
/// Using custom buffer size:
/// ```
/// # use fdf::cstr;
/// let custom: *const u8 = unsafe { cstr!(b"custom", 100) };
/// ```
///
/// Custom size with different pointer types:
/// ```
/// # use fdf::cstr;
/// let as_i8 = unsafe { cstr!(b"hello", 50) } as *const i8;
/// let as_u8 = unsafe { cstr!(b"world", 50) } as *const u8;
/// unsafe {
///     assert_eq!(*as_i8.offset(0), b'h' as i8);
///     assert_eq!(*as_u8.offset(0), b'w' as u8);
/// }
/// ```
///
/// ## Error cases
///
/// Debug mode length checking (default variant):
/// ```should_panic
/// # use fdf::cstr;
/// let long_string = [b'a'; 5000];
/// let will_crash:*const u8 = unsafe { cstr!(&long_string) };
/// ```
///
/// Debug mode length checking (custom variant):
/// ```should_panic
/// # use fdf::cstr;
/// let long_string = [b'a'; 100];
/// let will_crash_again:*const u8 = unsafe { cstr!(&long_string, 50) };
/// ```
///
/// # Notes
///
/// - Uses `AlignedBuffer` internally for proper alignment (rounds to next )
/// - In release builds, length checks are omitted for performance
/// - The pointer type is cast to the caller's expected type using `cast::<_>()`
/// - The buffer is automatically cleaned up when the scope exits
/// - For custom variant, the buffer size must be a compile-time constant
macro_rules! cstr {
    ($bytes:expr) => {{
        // Debug assert to check test builds for unexpected conditions
        // TODO! investigate this https://docs.rs/nix/latest/src/nix/lib.rs.html#318-350
        // Essentially I need to check the implications of this.
        core::debug_assert!($bytes.len() < $crate::LOCAL_PATH_MAX);
        // Create a buffer and make into a pointer
        let mut c_path_buf_start = $crate::AlignedBuffer::<u8, { $crate::LOCAL_PATH_MAX }>::new();
        let c_path_buf = c_path_buf_start.as_mut_ptr();

        // Copy the bytes into the buffer and append a null terminator
        core::ptr::copy_nonoverlapping($bytes.as_ptr(), c_path_buf, $bytes.len());
        // Write a null terminator at the end of the buffer
        c_path_buf.add($bytes.len()).write(0);
        //let caller choose cast
        c_path_buf.cast::<_>()
    }};
    // Secondary implementation for a custom length (eg, set to 256 for filenames when calling fstat)   (I don't like fstat!)
    ($bytes:expr, $n:expr) => {{


        core::debug_assert!($bytes.len() < $n); //Debug just for test builds, this is obviously a very unsafe macro!

        let mut c_path_buf_start = $crate::AlignedBuffer::<u8, { $n }>::new();
        let c_path_buf = c_path_buf_start.as_mut_ptr();

        core::ptr::copy_nonoverlapping($bytes.as_ptr(), c_path_buf, $bytes.len());
        c_path_buf.add($bytes.len()).write(0);

        c_path_buf.cast::<_>()
    }};




}

#[doc(hidden)]
/// A macro to skip . and .. entries when traversing a directory.
///
/// ## Usage
/// ```ignore
/// skip_dot_or_dot_dot_entries!(entry, continue);
/// ```
///
/// Takes:
/// - `$entry`: pointer to a dirent struct
/// - `$action`: a control-flow statement (e.g., `continue`, `break`, `return ...`)
///
/// Handles Linux vs BSD vs others and optional field differences.
macro_rules! skip_dot_or_dot_dot_entries {
    ($entry:expr, $action:expr) => {{
        #[allow(unused_unsafe)]
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        // SAFETY: when calling this macro, the pointer has already been ensured to be non null
        unsafe {
            match access_dirent!($entry, d_type) {
                //get the file type from my macro
                libc::DT_DIR | libc::DT_UNKNOWN => {
                    #[cfg(target_os = "linux")]
                    {
                        // Linux optimisation: reclen is always 24 for . and .. on linux
                        if access_dirent!($entry, d_reclen) == 24 {
                            let name_ptr = access_dirent!($entry, d_name);
                            match (*name_ptr.add(0), *name_ptr.add(1), *name_ptr.add(2)) {
                                (b'.', 0, _) | (b'.', b'.', 0) => $action,
                                _ => (),
                            }
                        }
                    }

                    #[cfg(not(target_os = "linux"))]
                    {
                        let name_ptr = access_dirent!($entry, d_name);
                        match (*name_ptr.add(0), *name_ptr.add(1), *name_ptr.add(2)) {
                            (b'.', 0, _) | (b'.', b'.', 0) => $action,
                            _ => (),
                        }
                    }
                }
                // For all other file types, no action needed
                _ => (),
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
        if $self
            .search_config
            .depth
            .is_some_and(|depth| $dir.depth >= depth.into())
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

macro_rules! send_files_if_not_empty {
    ($self:expr, $files:expr, $sender:expr) => {
        if !$files.is_empty() {
            if let Err(e) = $sender.send($files) {
                if $self.search_config.show_errors {
                    eprintln!("Error sending files: {e}");
                }
            }
        }
    };
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


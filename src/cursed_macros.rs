#![allow(unused_macros)]

#[allow(clippy::doc_markdown)]
#[macro_export(local_inner_macros)]
///A helper macro to safely access dirent(64 on linux)'s
/// fields of a `libc::dirent`/`libc::dirent64` aka 'dirent-type' struct by offset.
///
/// # Safety
/// - The caller must ensure that the pointer is valid and points to a 'dirent-type' struct.
/// - The field name must be a valid field of the 'dirent-type' struct.
///
/// # Field Aliases
/// - On BSD systems (FreeBSD, OpenBSD, NetBSD, DragonFly), `d_ino` is aliased to `d_fileno`
///   Example: `offset_dirent!(entry_ptr, d_ino)` -> aliases to `d_fileno` and returns the VALUE of an inode(u64)  (internal consistency, be glad it works!)
/// - On Linux, `d_reclen` is used to access the record length directly,
///  Example: `offset_dirent!(entry_ptr, d_reclen)`
/// - On MacOS/BSD, `d_namlen` is used to access the name length directly, this is a special case, since it is not aligned  similarly to `d_reclen`.
///  the other fields are accessed normally, as raw pointers to the field
/// /// # Usage
/// ```ignore
/// let entry_ptr: *const libc::dirent = ...; // Assume this is a valid pointer to a dirent struct
/// let d_name_ptr:*const _ = offset_dirent!(entry_ptr, d_name);
/// let d_reclen:usize = offset_dirent!(entry_ptr, d_reclen);
///
/// let d_namlen:usize = offset_dirent!(entry_ptr, d_namlen); // This is a special case for BSD and MacOS, where d_namlen is available
/// let d_ino :u64= offset_dirent!(entry_ptr, d_ino); // This
macro_rules! offset_dirent {
    // Special case for `d_reclen`
    ($entry_ptr:expr, d_reclen) => {{
        // SAFETY: Caller must ensure pointer is valid
        (*$entry_ptr).d_reclen as usize // /return usize
    }};
     // Special case for `d_namlen`


    ($entry_ptr:expr, d_namlen) => {{
        // SAFETY: Caller must ensure pointer is valid
        (*$entry_ptr).d_namlen as usize // access field directly as it is not aligned like the others
    }};//should this backup to a function call for platforms without d_namlen? TODO


    ($entry_ptr:expr, d_off) => {{
        // SAFETY: Caller must ensure pointer is valid
        (*$entry_ptr).d_off
    }};
    ($entry_ptr:expr,d_name) => {{
        //see reference https://github.com/rust-lang/rust/blob/8712e4567551a2714efa66dac204ec7137bc5605/library/std/src/sys/fs/unix.rs#L740
        //explanation also copypasted below.
         (&raw const (*$entry_ptr).d_name).cast::<u8>() //we have to have treat  pointer  differently because it's not guaranteed to actually be [0,256] (can't be worked with by value!)
    }};
      // Handle inode number field with aliasing for BSD systems
    //you can use d_ino or d_fileno (preferentially d_ino for cross-compatbility!)
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

    // General case for all other fields
    //which shouldn't really have any use case
    ($entry_ptr:expr, $field:ident) => {{ (*$entry_ptr).$field }};
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

#[macro_export]
/// A macro to create a C-style *str pointer from a byte slice (does not allocate!)
/// Returns a pointer to a null-terminated C-style *const _ (type inferred by caller, i8 or u8)
///
/// # Examples
///
///
/// Works with different pointer types:
/// ```
/// # use fdf::cstr;
/// let as_i8 = unsafe{ cstr!(b"hello")} as *const i8;
/// let as_u8 = unsafe{cstr!(b"world")} as *const u8;
/// unsafe {
///     assert_eq!(*as_i8.offset(0), b'h' as i8);
///     assert_eq!(*as_u8.offset(0), b'w' as u8);
/// }
/// ```
///
/// Proper null termination:
/// ```
/// # use fdf::cstr;
/// let s:*const u8 = unsafe{cstr!(b"test")};
/// unsafe {
///     assert_eq!(*s.offset(0), b't' );
///     assert_eq!(*s.offset(1), b'e' );
///     assert_eq!(*s.offset(2), b's' );
///     assert_eq!(*s.offset(3), b't' );
///     assert_eq!(*s.offset(4), 0);
/// }
/// ```
///
/// Empty string case:
/// ```
/// # use fdf::cstr;
/// let empty:*const u8 = unsafe{cstr!(b"")};
/// unsafe {
///     assert_eq!(*empty.offset(0), 0);
/// }
/// ```
///
/// The macro will panic in debug mode if the input exceeds LOCAL_PATH_MAX: (Simplified example)
/// ```should_panic
/// # use fdf::cstr;
/// let long_string = [b'a'; 5000];
/// let will_crash:*const u8 = unsafe{cstr!(&long_string)}; // will crash yay!
/// ```
macro_rules! cstr {
    ($bytes:expr) => {{
        // Debug assert to check test builds for unexpected conditions
        core::debug_assert!($bytes.len() < $crate::LOCAL_PATH_MAX);
        // Create a buffer and make into a pointer
        let mut c_path_buf_start = $crate::PathBuffer::new();
        let c_path_buf = c_path_buf_start.as_mut_ptr();

        // Copy the bytes into the buffer and append a null terminator
        std::ptr::copy_nonoverlapping($bytes.as_ptr(), c_path_buf, $bytes.len());
        // Write a null terminator at the end of the buffer
        c_path_buf.add($bytes.len()).write(0);
        //let caller choose cast
        c_path_buf.cast::<_>()
    }};
}

#[doc(hidden)]
#[allow(clippy::too_long_first_doc_paragraph)]
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
        unsafe {
            let d_type = offset_dirent!($entry, d_type);

            #[cfg(target_os = "linux")]
            {
                //reclen is always 24 for . and .. on linux,
                if (d_type == libc::DT_DIR || d_type == libc::DT_UNKNOWN)
                    && offset_dirent!($entry, d_reclen) == 24
                {
                    let name_ptr = offset_dirent!($entry, d_name);
                    match (*name_ptr.add(0), *name_ptr.add(1), *name_ptr.add(2)) {
                        (b'.', 0, _) | (b'.', b'.', 0) => $action,
                        _ => (),
                    }
                }
            }

            #[cfg(not(target_os = "linux"))]
            {
                if d_type == libc::DT_DIR || d_type == libc::DT_UNKNOWN {
                    let name_ptr = offset_dirent!($entry, d_name);
                    match (*name_ptr.add(0), *name_ptr.add(1), *name_ptr.add(2)) {
                        (b'.', 0, _) | (b'.', b'.', 0) => $action,
                        _ => (),
                    }
                }
            }
        }
    }};
}

#[macro_export]
/// Macro to implement `BytesStorage` for types that support `From<&[u8]>`
///The types must implement `From<&[u8]>` to be used with this macro
macro_rules! impl_bytes_storage {
    ($($type:ty),*) => {
        $(
            impl $crate::BytesStorage for $type {
                #[inline]
                fn from_slice(bytes: &[u8]) -> Self {
                    bytes.into()
                }
            }
        )*
    };
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

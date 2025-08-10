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

#[macro_export(local_inner_macros)]
/// A macro to create a C-style *str pointer from a byte slice(does not allocate!)
/// Returns a pointer to a null-terminated C-style *const _ (type inferred by caller, i8 or u8)
///
/// The argument should be a byte slice that c omes from a filesystem (so it's automatically under the stack size)

/// so eg `libc::open(cstr!(b"/"),libc::O_RDONLY)`
/// This macro takes a byte slice and returns a pointer to a null-terminated C-style string(either const i8/u8)
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

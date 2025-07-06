#![allow(unused_macros)]

//i didnt want to to use this macro but it saved a LOT of hassle/boilerplate. (vlight depdendency
//might remove this when less lazy.
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
/// - On Linux, `d_reclen` is used to access the record length directly, this is a special case, since it is not aligned like the others.
///  Example: `offset_dirent!(entry_ptr, d_reclen)` -> returns the record length as usize (internal consistency, be glad it works!)
/// - On MacOS/BSD, `d_namlen` is used to access the name length directly, this is a special case, since it is not aligned  similarly to `d_reclen`.
///  the other fields are accessed normally, as raw pointers to the field
/// /// # Usage
/// ```ignore
/// let entry_ptr: *const libc::dirent = ...; // Assume this is a valid pointer to a dirent struct
/// let d_name_ptr:*const _ = offset_dirent!(entry_ptr, d_name);
/// let d_reclen:usize = offset_dirent!(entry_ptr, d_reclen);
///
/// let d_namlen:usize = offset_dirent!(entry_ptr, d_namlen); // This is a special case for BSD and MacOS, where d_namlen is available
/// let d_ino_ptr :u64= offset_dirent!(entry_ptr, d_ino); // This
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
    //probably not as it would be inconsistent with the rest of the macro
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
            &raw const (*$entry_ptr).d_fileno as u64
        }

        #[cfg(not(any(
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        )))]
        {
            // SAFETY: Caller must ensure pointer is valid
            &raw const (*$entry_ptr).d_ino  as u64
        }
    }};

    // General case for all other fields
    ($entry_ptr:expr, $field:ident) => {{ &raw const (*$entry_ptr).$field }};
}

#[macro_export(local_inner_macros)]
/// A macro to create a C-style *str pointer from a byte slice(does not allocate!)
/// Returns a pointer to a null-terminated C-style *const _ (type inferred by caller, i8 or u8)
///
/// The first argument should be a byte slice
/// the second argument is optional as specifies a custom buffer size.
/// `cstr!(b"/home/sir_galahad", 256)`
/// so eg `libc::open(cstr!(b"/"),libc::O_RDONLY)`
/// or eg `libc::open(cstr!(b"/", 256),libc::O_RDONLY)`
/// This macro takes a byte slice and returns a pointer to a null-terminated C-style string.
macro_rules! cstr {
    ($bytes:expr) => {{
        // Debug assert to check test builds for unexpected conditions
        // Create a buffer and make into a pointer
        let c_path_buf = $crate::PathBuffer::new().as_mut_ptr();
        // Copy the bytes into the buffer and append a null terminator
        std::ptr::copy_nonoverlapping($bytes.as_ptr(), c_path_buf, $bytes.len());
        // Write a null terminator at the end of the buffer
        c_path_buf.add($bytes.len()).write(0);
        //let caller choose cast
        c_path_buf.cast::<_>()
    }};
    ($bytes:expr,$n:expr) => {{
        // create an uninitialised u8 slice and grab the pointer mutably  and make into a pointer
        let c_path_buf = $crate::AlignedBuffer::<u8, $n>::new().as_mut_ptr();
        // Copy the bytes into the buffer and append a null terminator
        std::ptr::copy_nonoverlapping($bytes.as_ptr(), c_path_buf, $bytes.len());
        c_path_buf.add($bytes.len()).write(0);

        c_path_buf.cast::<_>()
    }};
}

#[doc(hidden)]
#[allow(clippy::too_long_first_doc_paragraph)]
/// NOT INTENDED FOR PUBLIC USE, WILL BE PRIVATE SOON.
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
                if (*d_type == libc::DT_DIR || *d_type == libc::DT_UNKNOWN)
                    && offset_dirent!($entry, d_reclen) == 24
                {
                    let name_ptr = offset_dirent!($entry, d_name) as *const u8;
                    match (*name_ptr.add(0), *name_ptr.add(1), *name_ptr.add(2)) {
                        (b'.', 0, _) | (b'.', b'.', 0) => $action,
                        _ => (),
                    }
                }
            }

            #[cfg(not(target_os = "linux"))]
            {
                if *d_type == libc::DT_DIR || *d_type == libc::DT_UNKNOWN {
                    let name_ptr = offset_dirent!($entry, d_name) as *const u8;
                    match (*name_ptr.add(0), *name_ptr.add(1), *name_ptr.add(2)) {
                        (b'.', 0, _) | (b'.', b'.', 0) => $action,
                        _ => (),
                    }
                }
            }
        }
    }};
}

/// not public.
/// Constructs a path from the base path and the name pointer, returning a  slice of the full path
macro_rules! construct_path {
    ($self:ident, $dirent:ident) => {{


        let d_name = offset_dirent!($dirent, d_name) as *const u8;//cast as we need to use it as a pointer (it's in bytes now which is what we want)
        let base_len= $self.file_name_index(); //get the base path length, this is the length of the directory path

        let name_len = {
         #[cfg(target_os = "linux")]
        {   use $crate::dirent_const_time_strlen;
            dirent_const_time_strlen($dirent) //const time strlen for linux (specialisation)
        }

        #[cfg(any(
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly",
            target_os = "macos"
        ))]
        {
            offset_dirent!($dirent, d_namlen) //specialisation for BSD and macOS, where d_namlen is available
        }

        #[cfg(not(any(
           target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly",
            target_os = "macos"
        )))]
        {   use $crate::strlen;
             unsafe{strlen(offset_dirent!($dirent, d_name).cast::<_>())}
            // Fallback for other OSes
        }
            };




        std::ptr::copy_nonoverlapping(d_name,$self.path_buffer.as_mut_ptr().add(base_len),name_len,
        );

       $self.path_buffer.get_unchecked(..base_len+name_len)

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
/// const_from_env!(MYVAR: usize = "NYVAR", "6969");
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
/// // Creates a constant with documentation
/// const_from_env!(
///     /// Maximum path length for local filesystem operations
///     /// Default: 4096 (typical Linux PATH_MAX)
///     LOCAL_PATH_MAX: usize = "`LOCAL_PATH_MAX`", "4096"
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
            // Manual parsing for primitive types
            const fn parse_env<const N: usize>(s: &[u8]) -> $t {
                let mut i = 0;
                let mut n = 0;

                while i < s.len() {
                    let b = s[i];
                    match b {
                        b'0'..=b'9' => {
                            n = n * 10 + (b - b'0') as $t;
                        }
                        _ => panic!(concat!("Invalid ", stringify!($t), " value")),
                    }
                    i += 1;
                }
                n
            }

            // Handle the env var
            const VAL: &str = match option_env!($env) {
                Some(val) => val,
                None => $default,
            };
            parse_env::<{ VAL.len() }>(VAL.as_bytes())
        };
    };
}

//the below 2 macros are needed due to the fact we have 3 types of iterators, this makes it a lot cleaner!

/// Constructs a `DirEntry<S>` from a `dirent64`/`dirent` pointer for any relevant self type
/// Needed to be done via macro to avoid issues with duplication/mutability of structs
macro_rules! construct_dirent {
    ($self:ident, $dirent:ident) => {{
        let (d_type, inode) = unsafe {
            (
                *offset_dirent!($dirent, d_type), // get d_type
                offset_dirent!($dirent, d_ino),   // get inode
            )
        };

        let full_path = unsafe { construct_path!($self, $dirent) };
        let file_type = $crate::FileType::from_dtype_fallback(d_type, full_path);

        $crate::DirEntry {
            path: full_path.into(),
            file_type,
            inode,
            depth: $self.parent_depth + 1,
            file_name_index: $self.file_name_index,
        }
    }};
}

/// Constructs a temporary `TempDirent<S>` from a `dirent64`/`dirent` pointer for any relevant self type
/// This is used to filter entries without allocating memory on the heap.
/// It is a temporary structure that is used to filter entries before they are converted to `DirEntry<S>`.
/// Needed to be done via macro to avoid issues with duplication/mutability of structs
///
//not used YET
macro_rules! construct_temp_dirent {
    ($self:ident, $dirent:ident) => {{
        let (d_type, inode) = unsafe {
            (
                *offset_dirent!($dirent, d_type), // get d_type
                offset_dirent!($dirent, d_ino),   // get inode
            )
        };

        let full_path = unsafe { construct_path!($self, $dirent) };
        let file_type = $crate::FileType::from_dtype_fallback(d_type, full_path);

        $crate::TempDirent {
            path: full_path,
            file_type,
            inode,
            depth: $self.parent_depth + 1,
            file_name_index: $self.file_name_index,
            _marker: std::marker::PhantomData,
        }
    }};
}

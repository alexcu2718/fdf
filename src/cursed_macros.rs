#![allow(clippy::doc_markdown)]
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
macro_rules! offset_ptr {
    // Special case for `d_reclen`
    ($entry_ptr:expr, d_reclen) => {{
        // SAFETY: Caller must ensure pointer is valid
        (*$entry_ptr).d_reclen as usize // access field directly as it is not aligned like the others
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
/// let mypointer:*const u8= cstr!(b"/home/sir_galahad", 256);
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
        // Debug assert to check test builds for unexpected conditions
        debug_assert!($bytes.len() < $n, "Input too large for buffer");

        // create an uninitialised u8 slice and grab the pointer mutably  and make into a pointer
        let c_path_buf = $crate::AlignedBuffer::<u8, $n>::new().as_mut_ptr();
        // Copy the bytes into the buffer and append a null terminator
        std::ptr::copy_nonoverlapping($bytes.as_ptr(), c_path_buf, $bytes.len());
        c_path_buf.add($bytes.len()).write(0);

        c_path_buf.cast::<_>()
    }};
}

#[macro_export]
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
            let d_type = offset_ptr!($entry, d_type);
            let name_ptr = offset_ptr!($entry, d_name) as *const u8;

            #[cfg(target_os = "linux")]
            {
                let reclen = offset_ptr!($entry, d_reclen);
                if (*d_type == libc::DT_DIR || *d_type == libc::DT_UNKNOWN) && reclen == 24 {
                    match (*name_ptr.add(0), *name_ptr.add(1), *name_ptr.add(2)) {
                        (b'.', 0, _) | (b'.', b'.', 0) => $action,
                        _ => (),
                    }
                }
            }

            #[cfg(not(target_os = "linux"))]
            {
                if *d_type == libc::DT_DIR || *d_type == libc::DT_UNKNOWN {
                    match (*name_ptr.add(0), *name_ptr.add(1), *name_ptr.add(2)) {
                        (b'.', 0, _) | (b'.', b'.', 0) => $action,
                        _ => (),
                    }
                }
            }
        }
    }};
}

//SADLY ALTHOUGH THE TWO MACROS BELOW LOOK SIMILAR, THEY CANNOT BE USED EQUIVALENTLY

#[macro_export]
/// initialises a path buffer for syscall operations,
// appending a slash/null terminator (if it's a directory etc)
/// returns a tuple containing the length of the path and the `PathBuffer` itself.
macro_rules! init_path_buffer {
    ( $dir_path:expr) => {{
        let mut start_buffer=$crate::PathBuffer::new(); //create a new path buffer, this is a zeroed buffer of size `LOCAL_PATH_MAX
        let buffer_ptr = start_buffer.as_mut_ptr(); //get the mutable pointer to the buffer
        let mut base_len=$dir_path.len(); //get the length of the directory path, this is the length of the directory path
        let needs_slash = (($dir_path.depth() != 0) as u8) | (($dir_path.as_bytes() != b"/") as u8); //check if we need to append a slash(bitmasking it to 0 or 1)
        std::ptr::copy_nonoverlapping($dir_path.as_ptr(), buffer_ptr, base_len);
        *buffer_ptr.add(base_len) = (b'/') * needs_slash; //add slash or null terminator appropriately (completely deterministic)
        base_len += needs_slash as usize; //increment the base_len length by 1 if we added a slash, otherwise it stays the same
        (base_len,start_buffer)
    }};
}

///not intended for public use, will be private when boilerplate is donel
/// Constructs a path from the base path and the name pointer, returning a  slice of the full path
#[macro_export(local_inner_macros)]
#[allow(clippy::too_long_first_doc_paragraph)] //i like monologues, ok?
macro_rules! construct_path {
    ($self:ident, $dirent:ident) => {{


        let d_name = offset_ptr!($dirent, d_name) as *const u8;//cast as we need to use it as a pointer (it's in bytes now which is what we want)
        let base_len= $self.base_len(); //get the base path length, this is the length of the directory path

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
            offset_ptr!($dirent, d_namlen) //specialisation for BSD and macOS, where d_namlen is available
        }

        #[cfg(not(any(
           target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly",
            target_os = "macos"
        )))]
        {
             libc::strlen(offset_ptr!($dirent, d_name) as *const _)
            // Fallback for other OSes, using libc::strlen because i cant cover every edge case...
        }
            };




        std::ptr::copy_nonoverlapping(d_name,$self.path_buffer.as_mut_ptr().add(base_len),name_len,
        );

       $self.path_buffer.get_unchecked(..base_len+name_len)

    }};
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[macro_export(local_inner_macros)]
/// Prefetches the next likely entry in the buffer, basically trying to keep cache warm
macro_rules! prefetch_next_entry {
    ($self:ident) => {
        //we know it's going to be accessed soon, since reclen(size of the entry) is often 40 or below, this seems a good compromise.
        if $self.offset + 128 < $self.remaining_bytes as usize {
            unsafe {
                use std::arch::x86_64::{_MM_HINT_T0, _mm_prefetch};
                let next_entry = $self.buffer.as_ptr().add($self.offset + 64).cast();
                _mm_prefetch(next_entry, _MM_HINT_T0);// bvvvvvvvv333333333333 CAT DID THIS IM LK\\\Z//im leaving this art
            }
        }
    };
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[macro_export]
/// Prefetches the next buffer for reading, this is used to keep the cache warm for the next read operation
macro_rules! prefetch_next_buffer {
    ($self:ident) => {
        unsafe {
            use std::arch::x86_64::{_MM_HINT_T0, _mm_prefetch};
            _mm_prefetch($self.buffer.as_ptr().cast(), _MM_HINT_T0);
        }
    };
}

#[macro_export]
#[allow(clippy::ptr_as_ptr)]
/// A high-performance, SIMD-accelerated `strlen` for null-terminated strings.
///
/// Uses **AVX2 (32-byte vectors)** if available, otherwise **SSE2 (16-byte)**, and falls back to `libc::strlen` if no SIMD is supported.
///
/// # Safety
/// - **`ptr` must be a valid, non-null pointer to a null-terminated string.**
/// - **Does not check if the string starts with a null terminator** (unlike `libc::strlen`).
/// - **Uses unaligned loads** (`_mm_loadu_si128`/`_mm256_loadu_si256`), so alignment is not required.
/// - WILL NOT DETECT THE IF THE NULL TERMINATOR IS MISSING/FIRST BYTE IS NULL.
/// - **May read beyond the end of the string** until it finds a null terminator
macro_rules! strlen_asm {
    ($ptr:expr) => {{
        #[cfg(all(
            target_arch = "x86_64",
            any(target_feature = "avx2", target_feature = "sse2")
        ))]
        {
            // SAFETY: Caller must ensure `ptr` is valid and null-terminated.

            #[cfg(target_feature = "avx2")]
            {
                use std::arch::x86_64::{
                    __m256i,
                    _mm256_cmpeq_epi8,    // Compare 32 bytes at once
                    _mm256_loadu_si256,   // Unaligned 32-byte load
                    _mm256_movemask_epi8, // Bitmask of null matches
                    _mm256_setzero_si256, // Zero vector
                };

                let mut offset = 0;
                loop {
                    let chunk = _mm256_loadu_si256($ptr.add(offset) as *const __m256i);
                    let zeros = _mm256_setzero_si256();
                    let cmp = _mm256_cmpeq_epi8(chunk, zeros);
                    let mask = _mm256_movemask_epi8(cmp) as i32;

                    if mask != 0 {
                        break offset + mask.trailing_zeros() as usize;
                    }
                    offset += 32; // Process next 32-byte chunk
                }
            }

            #[cfg(not(target_feature = "avx2"))]
            {
                use std::arch::x86_64::{
                    __m128i,
                    _mm_cmpeq_epi8,    // Compare 16 bytes
                    _mm_loadu_si128,   // Unaligned 16-byte load
                    _mm_movemask_epi8, // Bitmask of null matches
                    _mm_setzero_si128, // Zero vector
                };

                let mut offset = 0;
                loop {
                    let chunk = _mm_loadu_si128($ptr.add(offset) as *const __m128i);
                    let zeros = _mm_setzero_si128();
                    let cmp = _mm_cmpeq_epi8(chunk, zeros);
                    let mask = _mm_movemask_epi8(cmp) as i32;

                    if mask != 0 {
                        break offset + mask.trailing_zeros() as usize;
                    }
                    offset += 16; // Process next 16-byte chunk
                }
            }
        }

        #[cfg(not(all(
            target_arch = "x86_64",
            any(target_feature = "avx2", target_feature = "sse2")
        )))]
        {
            // Fallback to libc::strlen if no SIMD support
            libc::strlen($ptr.cast::<_>())
        }
    }};
}

#[macro_export]
// Macro to implement BytesStorage for types that support `From<&[u8]>`
//The types must implement `From<&[u8]>` to be used with this macro
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

/// Macro to create a const from an env var with compile-time parsing
/// const_from_env!(LOCAL_PATH_MAX: usize = "LOCAL_PATH_MAX", "X");, where X(usize) is the default value if the env var is not set
///
/// I realise people could have massive filesystems, i should probably write a rebuild script on value change.TODO!
#[macro_export]
macro_rules! const_from_env {
    ($name:ident: $t:ty = $env:expr, $default:expr) => {
        pub const $name: $t = {
            // Manual parsing for primitive types
            //we have to assume it's indexed basically in order to be const
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

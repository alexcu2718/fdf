#![allow(clippy::doc_markdown)]
#[macro_export]
///A modified helper macro from the standard library to safely access(as per my opinion i guess...)
/// fields of a `libc::dirent64` struct by offset.
///
/// It has been modified to handle the `d_reclen` field differently due to its alignment issues, handling the issue subtly for the user.
/// This macro is used to access fields of a `libc::dirent64` struct by offset, allowing for more flexible and efficient access.
/// It takes a pointer to a `libc::dirent64` struct and a field name, and returns a pointer to the field.
/// The field name must be a valid field of the `libc::dirent64` struct.
/// The macro uses `std::mem::offset_of!` to calculate the offset of the field in the struct.
/// /// # Safety
/// - The caller must ensure that the pointer is valid and points to a `libc::dirent64` struct.
/// - The field name must be a valid field of the `libc::dirent64` struct.
macro_rules! offset_ptr {
    // Special case for d_reclen
    // I recommend casting to usize.
    ($entry_ptr:expr, d_reclen) => {{
        // SAFETY: Caller must ensure pointer is valid
        (*$entry_ptr).d_reclen //access field directly as it is not aligned like the others
    }};

    // General case for all other fields
    ($entry_ptr:expr, $field:ident) => {{
        const OFFSET: isize = std::mem::offset_of!(libc::dirent64, $field) as isize;

        if true {
            // Normal field access via offset
            // SAFETY: Caller must ensure pointer is valid
            $entry_ptr.byte_offset(OFFSET) as _
        } else {
            // Type inference branch (never executed)
            #[allow(deref_nullptr, unused_unsafe)]
            unsafe {
                std::ptr::addr_of!((*std::ptr::null::<libc::dirent64>()).$field)
            }
        }
    }};
}


#[macro_export]
/// A macro to create a C-style string pointer from a byte slice, the first argument should be a byte slice
/// the second argument is optional as specifies a custom buffer size to use
/// so eg `libc::open(cstr!(b"/"),libc::O_RDONLY)`
/// or eg `libc::open(cstr!(b"/", 256),libc::O_RDONLY)`
/// This macro takes a byte slice and returns a pointer to a null-terminated C-style string.
macro_rules! cstr {
    ($bytes:expr) => {{
        // Debug assert to check test builds for unexpected conditions
        debug_assert!(
            $bytes.len() < $crate::LOCAL_PATH_MAX,
            "Input too large for buffer"
        );

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
#[allow(clippy::too_long_first_doc_paragraph)]
///NOT INTENDED FO FOR PUBLIC USE, WILL BE PRIVATE SOON.
/// A macro to skip . and .. entries when traversing
///
/// Takes 2 mandatory args:
/// - `d_type`: The directory entry type (e.g., `(*dirnt).d_type`)
/// - `name_ptr`: Pointer to the entry name
///
/// And 1 optional arg: (the reclen)
///
/// - `reclen`: If provided, also checks that reclen == 24
///  when testing directory entries, this helps to reduce any checking pointers
macro_rules! skip_dot_entries {
    // Version with reclen check
    ($d_type:expr, $name_ptr:expr, $reclen:expr) => {{
        #[allow(clippy::macro_metavars_in_unsafe)]
        unsafe {
            //check dtyype first, only 10% of files are directories, then unknown, etc
            if ($d_type == libc::DT_DIR || $d_type == libc::DT_UNKNOWN) && $reclen == 24 {
                match (*$name_ptr.add(0), *$name_ptr.add(1), *$name_ptr.add(2)) {
                    (b'.', 0, _) | (b'.', b'.', 0) => continue, //if it is . or .., skip it
                    _ => (),
                }
            }
        }
    }};

    // Version without reclen check
    ($d_type:expr, $name_ptr:expr) => {{
        #[allow(clippy::macro_metavars_in_unsafe)]
        unsafe {
            if $d_type == libc::DT_DIR || $d_type == libc::DT_UNKNOWN {
                //no reclen check based on user preference
                match (*$name_ptr.add(0), *$name_ptr.add(1), *$name_ptr.add(2)) {
                    (b'.', 0, _) | (b'.', b'.', 0) => continue,
                    _ => (),
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
macro_rules! init_path_buffer_syscall {
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

#[macro_export(local_inner_macros)]
#[allow(clippy::too_long_first_doc_paragraph)]
/// initialises a path buffer for readdir operations.
/// the macro appends a slash/null terminator if necessary and returns  `PathBuffer` with the base path+filename
macro_rules! init_path_buffer_readdir {
    ($dir_path:expr) => {{
        let mut buffer = $crate::PathBuffer::new(); //see above comments.
        let dirp = $dir_path.as_bytes();
        let dirp_len = dirp.len();
        let needs_slash = ($dir_path.depth != 0) as u8 | ((dirp != b"/") as u8);
        let buffer_ptr = buffer.as_mut_ptr();
        std::ptr::copy_nonoverlapping(dirp.as_ptr(), buffer_ptr, dirp_len);
        *buffer_ptr.add(dirp_len) = (b'/') * needs_slash;
        buffer
    }};
}

///not intended for public use, will be private when boilerplate is done
/// a version of `construct_path!` that uses a constant time strlen macro to calculate the length of the name pointer
/// this is really only an intellectual thing+exercise in reducing branching+complexity. THEY NEED TO BE BENCHMARKED.
/// Constructs a path from the base path and the name pointer, returning a  slice of the full path
#[macro_export(local_inner_macros)]
#[allow(clippy::too_long_first_doc_paragraph)] //i like monologues, ok?
macro_rules! construct_path {
    ($self:ident, $dirent:ident) => {{
        let d_name = $crate::offset_ptr!($dirent, d_name) as *const u8;//cast as we need to use it as a pointer (it's in bytes now which is what we want)
        let base_len= $self.base_len as usize; //get the base path length, this is the length of the directory path
        let name_len = $crate::dirent_const_time_strlen($dirent);
        std::ptr::copy_nonoverlapping(d_name,$self.path_buffer.as_mut_ptr().add(base_len),name_len,
        );

       $self.path_buffer.get_unchecked(..base_len+name_len)

    }};
}

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "x86_64")]
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
            unsafe { libc::strlen($ptr.cast::<i8>()) }
        }
    }};
}

#[macro_export]
/// A macro to extract values from a `libc::dirent64` struct.
/// This macro returns a tuple containing:
/// - A pointer to the name field (null-terminated string) 'd_name' *const u8
/// - The file type 'd_type' as u8 (e.g., DT_REG, DT_DIR)
/// - The inode number 'd_ino' as u64
/// - The record length 'd_reclen' as usize
///  Optionally, a minimal version can be used that excludes the record length.
/// /// Usage:
/// use libc::dirent64;
/// use crate::get_dirent_vals;
/// let dirent: *const libc::dirent64 = todo!(); // Assume this is a valid pointer to a dirent64 struct
/// let (name_ptr, file_type, inode, reclen):(*const u8,u8,u64,usize) = get_dirent_vals!(dirent);
/// let (name_ptr, file_type, inode):(*const u8,u8,u64)  = get_dirent_vals!(@minimal dirent); // Minimal version without reclen
macro_rules! get_dirent_vals {
    ($d:expr) => {{
        // return relevant fields with type inferred by user

        unsafe {
            (
                // d_name: pointer to the name field (null-terminated string)
                $crate::offset_ptr!($d, d_name) as _, //let user determine type
                // d_type: file type (DT_REG, DT_DIR, etc.) this will be 0 if unknown/Filesystem doesnt give dtype, we have to call lstat then alas.
                *$crate::offset_ptr!($d, d_type) as _,
                 // d_ino: inode number (represents file unique id)
                *$crate::offset_ptr!($d, d_ino) as _,
                 // d_reclen: record length
                $crate::offset_ptr!($d,d_reclen) as _//this is not guaranteed to be aligned, my macro fixes this!

            )
        }
    }};
    (@minimal $d:expr) => {{
        //minimal version, as we don't need reclen for readdir,
        // Cast the dirent pointer to a byte pointer for offset calculations
        unsafe {
            (

                $crate::offset_ptr!($d, d_name) as _,
                *$crate::offset_ptr!($d, d_type) as _,
                 *$crate::offset_ptr!($d, d_ino) as _,
            )
        }
    }};
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

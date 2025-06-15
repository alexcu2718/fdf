#![allow(clippy::doc_markdown)]

#[allow(clippy::ptr_as_ptr)]
#[allow(clippy::too_long_first_doc_paragraph)]
#[macro_export]
///copied this macro from the standard library
///using it to access offsets in a more strict way, basically it's assumed the `libc::dirent64` struct is the same as the one in the standard library
/// this is used to get a pointer to a field in the `libc::dirent64` struct and avoid intermediate references
macro_rules! offset_ptr {
    ($entry_ptr:expr, $field:ident) => {{
        const OFFSET: isize = std::mem::offset_of!(libc::dirent64, $field) as isize;
        if true {
            // Cast to the same type determined by the else branch.

            $entry_ptr.byte_offset(OFFSET).cast::<_>()
        } else {
            #[allow(deref_nullptr)]
            {
                &raw const (*std::ptr::null::<libc::dirent64>()).$field
            }
        }
    }};
}

//a cheap debug print macro, only prints if debug_assertions is enabled
#[macro_export]
macro_rules! debug_print {
    ($expr:expr) => {
        #[cfg(debug_assertions)]
        {
            dbg!($expr);
        }
    };
}

#[macro_export]
/// A macro to create a C-style string pointer from a byte slice
macro_rules! cstr {
    ($bytes:expr) => {{
        // Debug assert to check test builds for unexpected conditions
        debug_assert!(
            $bytes.len() < $crate::LOCAL_PATH_MAX,
            "Input too large for buffer"
        );

        // Create a  and make into a pointer
        let c_path_buf = $crate::PathBuffer::new().as_mut_ptr();

       
        std::ptr::copy_nonoverlapping($bytes.as_ptr(), c_path_buf, $bytes.len());
        c_path_buf.add($bytes.len()).write(0);
        

        c_path_buf.cast::<_>()
    }};
}

#[macro_export]
#[allow(clippy::too_long_first_doc_paragraph)]
/// A version of `cstr!` that allows specifying a maximum length for the buffer, intended to be used publically
///so eg `libc::open(cstr_n!(b"/",2),libc::O_RDONLY)`
macro_rules! cstr_n {
    ($bytes:expr,$n:expr) => {{
        // Debug assert to check test builds for unexpected conditions
        debug_assert!($bytes.len() < $n, "Input too large for buffer");

        // create an uninitialised u8 slice and grab the pointer mutably  and make into a pointer
        let c_path_buf = $crate::AlignedBuffer::<u8, $n>::new().as_mut_ptr();

      
        std::ptr::copy_nonoverlapping($bytes.as_ptr(), c_path_buf, $bytes.len());
        c_path_buf.add($bytes.len()).write(0);
        

        c_path_buf.cast::<_>()
    }};
}

#[macro_export]
#[allow(clippy::too_long_first_doc_paragraph)]
/// A macro to skip . and .. entries when traversing
///
/// Takes 2 mandatory args:
/// - `d_type`: The directory entry type (e.g., `(*dirnt).d_type`)
/// - `name_ptr`: Pointer to the entry name
///
/// And 1 optional arg:
/// - `reclen`: If provided, also checks that reclen == 24 when testing directory entries
macro_rules! skip_dot_entries {
    // Version with reclen check
    ($d_type:expr, $name_ptr:expr, $reclen:expr) => {{
        #[allow(clippy::macro_metavars_in_unsafe)]
        unsafe {
            let ddd = ($d_type == libc::DT_DIR || $d_type == libc::DT_UNKNOWN) && $reclen == 24;
            if ddd && *$name_ptr.add(0) == 46 {  // 46 == '.' in ASCII
                if *$name_ptr.add(1) == 0 ||     // Single dot case
                   (*$name_ptr.add(1) == 46 &&  // Double dot case
                    *$name_ptr.add(2) == 0) {
                    continue;
                }
            }
        }
    }};

    // Version without reclen check
    ($d_type:expr, $name_ptr:expr) => {{
        #[allow(clippy::macro_metavars_in_unsafe)]
        unsafe {
            if ($d_type == libc::DT_DIR || $d_type == libc::DT_UNKNOWN) &&
               *$name_ptr.add(0) == 46 {
                if *$name_ptr.add(1) == 0 ||     // Single dot case
                   (*$name_ptr.add(1) == 46 &&  // Double dot case
                    *$name_ptr.add(2) == 0) {
                    continue;
                }
            }
        }
    }};
}

#[macro_export]
/// initialises a path buffer for syscall operations,
// appending a slash if necessary and returning a pointer to the buffer (the mutable ptr of the first argument).
macro_rules! init_path_buffer_syscall {
    ($path_buffer:expr, $path_len:ident, $dir_path:expr, $self:expr) => {{
        let buffer_ptr = $path_buffer.as_mut_ptr();

        // Branchless needs_slash calculation (returns 0 or 1)
        #[allow(clippy::cast_lossless)] //shutup
        let needs_slash = (($self.depth != 0) as u8) | (($dir_path != b"/") as u8);

        unsafe {
            // Copy directory path
            std::ptr::copy_nonoverlapping($dir_path.as_ptr(), buffer_ptr, $path_len);

            // Branchless slash writing and length adjustment, write a null terminator if no slash.
            *buffer_ptr.add($path_len) = (b'/') * needs_slash;
            $path_len += needs_slash as usize;
        }

        buffer_ptr
    }};
}

#[macro_export(local_inner_macros)]
#[allow(clippy::too_long_first_doc_paragraph)]
/// initialises a path buffer for readdir operations.
/// the macro appends a slash if necessary and returning the base length of the path.
/// Returns the base length of the path, which is the length of the directory
///  path plus one if a slash is needed(but also mutates the buffer invisibly, not ideal, i will change this.)
macro_rules! init_path_buffer_readdir {
    ($dir_path:expr, $buffer:expr) => {{
        let dirp = $dir_path.as_bytes();
        let dirp_len = dirp.len();

        // branchless needs_slash calculation (easier boolean shortcircuit on LHS)
        #[allow(clippy::cast_lossless)] //shutup
        let needs_slash = ($dir_path.depth != 0) as u8 | ((dirp != b"/") as u8);
        let base_len = dirp_len + needs_slash as usize;

      
        let buffer_ptr = $buffer.as_mut_ptr();

        // Copy directory path
        std::ptr::copy_nonoverlapping(dirp.as_ptr(), buffer_ptr, dirp_len);

            // branchless slash writing(we either write a slash or null terminator)
        *buffer_ptr.add(dirp_len) = (b'/') * needs_slash;
        

        base_len
    }};
}

#[macro_export]
/// Copies a null-terminated string into a buffer after a base offset
///
/// # Safety
/// - `name_file` must point to a valid null-terminated string
/// - `self` must have sufficient capacity for base_len + string length
macro_rules! copy_name_to_buffer {
    ($self:expr, $name_file:expr) => {{
        // Calculate available space after base_len
        let base_len = $self.base_len as usize;
        // Get string length using optimized SSE2 version
        let name_len =  $crate::strlen_asm!($name_file) ;
        //we avx2/sse2 ideally here, perfect for the likely size of it. I have considered
        //implemented a lot of these as macros to avoid function calls
        // SAFETY:
        // We've calculated the position of the null terminator.
    
         std::ptr::copy_nonoverlapping($name_file, $self.as_mut_ptr().add(base_len), name_len);
        

        base_len + name_len
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

///not intended for public use, will be private when boilerplate is done
/// Constructs a path from the base path and the name pointer, returning a  slice of the full path
#[macro_export(local_inner_macros)]
macro_rules! construct_path {
    ($self:ident, $name_ptr:ident) => {{
        let name_len = $crate::strlen_asm!($name_ptr);
        let total_len = $self.base_path_len as usize + name_len;
        std::ptr::copy_nonoverlapping(
            $name_ptr,
            $self
                .path_buffer
                .as_mut_ptr()
                .add($self.base_path_len as usize),
            name_len,
        );

        let full_path = $self.path_buffer.get_unchecked(..total_len);
        full_path
    }};
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
///
/// # Performance
/// - **Constant-time per 16/32-byte block(arch independent)** (no branching per byte).
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
                        _mm256_cmpeq_epi8,  // Compare 32 bytes at once
                        _mm256_loadu_si256, // Unaligned 32-byte load
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
                        _mm_cmpeq_epi8,  // Compare 16 bytes
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







///not intended for public use, will be private when boilerplate is done
/// a version of `construct_path!` that uses a constant time strlen macro to calculate the length of the name pointer
/// this is really only an intellectual thing+exercise in reducing branching+complexity. THEY NEED TO BE BENCHMARKED.
/// Constructs a path from the base path and the name pointer, returning a  slice of the full path
#[macro_export(local_inner_macros)]
#[allow(clippy::too_long_first_doc_paragraph)] //i like monologues, ok?
macro_rules! construct_path_optimised {
    ($self:ident, $dent:ident) => {{
        let name_ptr = $crate::offset_ptr!($dent, d_name).cast::<u8>();
        //let name_len = $crate::dirent_fixed_time_strlen!($dent);
        let name_len = $crate::dirent_const_time_strlen!($dent);
        let total_len = $self.base_path_len as usize + name_len;

        std::ptr::copy_nonoverlapping(
            name_ptr,
            $self
                .path_buffer
                .as_mut_ptr()
                .add($self.base_path_len as usize),
            name_len,
        );

        let full_path = $self.path_buffer.get_unchecked(..total_len);
        full_path
    }};
}

#[macro_export]
#[allow(clippy::too_long_first_doc_paragraph)] //i like monologues, ok?
/// The crown jewel of cursed macros(this is const, see the function equivalent for proof).
///
/// A macro to calculate the length of a directory entry name in constant/fixed time. (IDK!I STUDIED TOPOLOGY/CALCULUS INSTEAD)
/// We use bithacks to find the first null byte in the last u64 word of the `libc::dirent64` struct.
/// This macro can be used in in one way, when using readdir/getdents, to calculate the length of the d_name field in a `libc::dirent64` struct.
/// It returns the length of the name in bytes, excluding the null terminator.
/// Reference https://github.com/lattera/glibc/blob/master/string/strlen.c#L1
/// Reference https://graphics.stanford.edu/~seander/bithacks.html#HasZeroByte
/// Reference https://github.com/Soveu/find/blob/master/src/dirent.rs  (combining all these tricks, i made this beautiful thing)
macro_rules! dirent_const_time_strlen {
    ($dirent:expr) => {{
        const DIRENT_HEADER_SIZE: usize = std::mem::offset_of!(libc::dirent64, d_name) + 1;
        let reclen = (*$dirent).d_reclen as usize; // we MUST cast this way, as it is not guaranteed to be aligned, so we can't use offset_ptr!() here
        // Calculate the number of u64 words in the record length
        // Ensure that the record length is a multiple of 8 so we can cast to u64
        // Calculate last word(by indexing into the last 8 bytes of the dirent)
          let last_word = *(($dirent as *const u8).add(reclen - 8) as *const u64);
        // Special case: When processing the 3rd u64 word (index 2), we need to mask
        // the non-name bytes (d_type and padding) to avoid false null detection.
        // The 0x00FF_FFFF mask preserves only the 3 bytes where the name could start.
        // Branchless masking: avoids branching by using a mask that is either 0 or 0x00FF_FFFF
          #[allow(clippy::cast_lossless)] //shutup
        // Branchless 3rd-word mask (0x00FF_FFFF if index==2 else 0)
        let mask = 0x00FF_FFFFu64 * ((reclen / 8 == 3) as u64);// (multiply by 0 or 1)
        let zero_bit = (last_word | mask).wrapping_sub(0x0101_0101_0101_0101)
            & !(last_word | mask)
            & 0x8080_8080_8080_8080;

        reclen - DIRENT_HEADER_SIZE - (7 - (zero_bit.trailing_zeros() >> 3) as usize)
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
                $crate::offset_ptr!($d, d_name).cast::<_>(), //let user determine type
                // d_type: file type (DT_REG, DT_DIR, etc.) this will be 0 if unknown/Filesystem doesnt give dtype, we have to call lstat then alas.
                *$crate::offset_ptr!($d, d_type).cast::<_>(),
                 // d_ino: inode number (represents file unique id)
                *$crate::offset_ptr!($d, d_ino) as _,
                 // d_reclen: record length
                (*$d).d_reclen as _, //this is not guaranteed to be aligned as we need to treat it differently, we need to access it NOT through byte_offset

            )
        }
    }};
    (@minimal $d:expr) => {{
        //minimal version, as we don't need reclen for readdir,
        // Cast the dirent pointer to a byte pointer for offset calculations
        unsafe {
            (

                $crate::offset_ptr!($d, d_name).cast::<_>(),
                *$crate::offset_ptr!($d, d_type).cast::<_>(),
                 *$crate::offset_ptr!($d, d_ino) as _,
            )
        }
    }};
}

/// Macro to create a const from an env var with compile-time parsing
/// const_from_env!(LOCAL_PATH_MAX: usize = "LOCAL_PATH_MAX", "512");
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

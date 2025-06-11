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
        #[allow(unused_unsafe)] //macro collision i cant be bothered to fix rn
        // Copy bytes and null-terminate
        unsafe {
            std::ptr::copy_nonoverlapping($bytes.as_ptr(), c_path_buf, $bytes.len());
            c_path_buf.add($bytes.len()).write(0);
        }

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
        // #[allow(unused_unsafe)] //macro collision i cant be bothered to fix rn
        // Copy bytes and null-terminate
        unsafe {
            std::ptr::copy_nonoverlapping($bytes.as_ptr(), c_path_buf, $bytes.len());
            c_path_buf.add($bytes.len()).write(0);
        }

        c_path_buf.cast::<_>()
    }};
}

#[macro_export]
#[allow(clippy::too_long_first_doc_paragraph)]
///a macro to skip . and .. entries when traversing, takes 2 mandatory args, `d_type`,
/// which is if eg let dirnt:*const dirent64; then `d_type`=`(*dirnt).d_type`
//so it's expecting a `u8` basically. then it optionally takes offset and reclen, these are now deprecated but they were in use in a previous build
//ive kept them because naturally variadic macros will give no performance hit (Eg why this crate even exists)
macro_rules! skip_dot_entries {
    ($d_type:expr, $name_ptr:expr $(, $offset:expr, $reclen:expr)?) => {
        //ddd=indicator of whether the dent struct is dir/unknown, if it's unknown, we just need to check the pointer first index
        // which will eliminate 50%
       #[allow(clippy::macro_metavars_in_unsafe)]//stupid error let me use my hack macros.
        unsafe {
            let ddd = $d_type == libc::DT_DIR || $d_type == libc::DT_UNKNOWN;
            if ddd && *$name_ptr.add(0) == 46 {  // 46 == '.' in ASCII //access first element of pointer and dereference for value and check if its ascii . aka 46
                // Check for "." or ".."
                if *$name_ptr.add(1) == 0 ||     // Single dot case
                   *$name_ptr.add(1) == 46 &&   // Double dot case
                    *$name_ptr.add(2) == 0 {
                    $($offset += $reclen;)? //optional args
                    continue;
                }
            }
        }
    };
}

#[macro_export]
#[allow(clippy::too_long_first_doc_paragraph)]
//this isnt meant to be public, i cant be  bothered with the boilerplate, dunno, enjoy some unsafe code!
/// initialises a path buffer for syscall operations,
// appending a slash if necessary and returning a pointer to the buffer (the mutable ptr of the first argument).
macro_rules! init_path_buffer_syscall {
    ($path_buffer:expr, $path_len:ident, $dir_path:expr, $self:expr) => {{
        let buffer_ptr = $path_buffer.as_mut_ptr();
        let needs_slash = $self.depth != 0 || $dir_path != b"/"; //easier boolean shortcircuit on LHS

        unsafe {
            std::ptr::copy_nonoverlapping($dir_path.as_ptr(), buffer_ptr, $path_len);

            if needs_slash {
                buffer_ptr.add($path_len).write(b'/');
                $path_len += 1;
            }
        }

        buffer_ptr
    }};
}

#[macro_export]
#[allow(clippy::too_long_first_doc_paragraph)]
/// initialises a path buffer for readdir operations-
/// appending a slash if necessary and returning the base length of the path.
/// Returns the base length of the path, which is the length of the directory
///  path plus one if a slash is needed.
macro_rules! init_path_buffer_readdir {
    ($dir_path:expr, $buffer:expr) => {{
        let dirp = $dir_path.as_bytes();
        let dirp_len = dirp.len();
        let needs_slash = $dir_path.depth != 0 || dirp != b"/"; //easier boolean shortcircuit on LHS
        let base_len = dirp_len + needs_slash as usize;

        let buffer_ptr = $buffer.as_mut_ptr();

        unsafe {
            std::ptr::copy_nonoverlapping(dirp.as_ptr(), buffer_ptr, dirp_len);

            if needs_slash {
                buffer_ptr.add(dirp_len).write(b'/');
            }
        }

        base_len
    }};
}

#[macro_export]
#[allow(clippy::too_long_first_doc_paragraph)]
/// copies the name from the `name_file` pointer into the buffer of the `self` object, starting after the base length.
macro_rules! copy_name_to_buffer {
    ($self:expr, $name_file:expr) => {{
        let base_len = $self.base_len as usize;
        let name_len = unsafe { $crate::strlen_asm($name_file) };//we use specified repne scasb because its likely<=8bytes
        let name_bytes: &[u8] = unsafe { &*std::ptr::slice_from_raw_parts($name_file, name_len) };//no ub check suck it
        let total_path_len = base_len + name_len;

        unsafe {
            std::ptr::copy_nonoverlapping(
                name_bytes.as_ptr(),
                $self.as_mut_ptr().add(base_len),
                name_len,
            );
        }

        total_path_len
    }};
}

#[cfg(target_arch = "x86_64")]
#[macro_export]
/// Prefetches the next likely entry in the buffer, basically trying to keep cache warm
macro_rules! prefetch_next_entry {
    ($self:ident) => {
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
///not intended for public use, will be private when boilerplate is done
/// Constructs a path from the base path and the name pointer, returning a mutable slice of the full path
macro_rules! construct_path {
    ($self:ident, $name_ptr:ident) => {{

        let name_len = $crate::strlen_asm($name_ptr);
        let name_bytes = &*std::ptr::slice_from_raw_parts($name_ptr, name_len);
        let total_len = $self.base_path_len as usize + name_len;

        std::ptr::copy_nonoverlapping(
            name_bytes.as_ptr(),
            $self
                .path_buffer
                .as_mut_ptr()
                .add($self.base_path_len as usize),
            name_len,
        );

        let full_path = $self.path_buffer.get_unchecked_mut(..total_len);
        full_path
    }};
}


/* 

#[macro_export]
/// A macro to calculate the length of a directory entry name in constant time (SSE2 implementation is because it checks the entire 8 byte array in 1 op). 
/// This macro can be used in two ways:
/// 1. With a single argument: `dirent_const_time_strlen!(dirent)`, where `dirent` is a pointer to a `libc::dirent64` struct.
/// 2. With two arguments: `dirent_const_time_strlen!(dirent, reclen)`, where `reclen` is the record length of the directory entry.
/// 3. The only point in two arguments is to avoid recalculation(altho trivial) and to allow a custom record length to be used.
macro_rules! dirent_const_time_strlen {
    // Single argument version (gets reclen from dirent)
    ($dirent:expr) => {{
        let reclen = *offset_ptr!($dirent, d_reclen) as usize ;
        dirent_const_time_strlen!($dirent, reclen) //this felt so good to do
    }};

    // Two argument version (dirent + reclen)
    ($dirent:expr, $reclen:expr) => {{
        #[allow(clippy::integer_division_remainder_used)]
        #[allow(clippy::ptr_as_ptr)]
        #[allow(clippy::integer_division)]
        #[allow(clippy::items_after_statements)]
        #[allow(clippy::little_endian_bytes)]


        let reclen_in_u64s = $reclen / 8;
        // Ensure that the record length is a multiple of 8 so we can cast to u64
        //reclen is always a multiple of 8, so this is safe for the next step
        debug_assert!($reclen % 8 == 0, "reclen={} is not a multiple of 8", $reclen);
        debug_assert!($reclen >= 16, "reclen={} is greater than 16", $reclen);
        // Treat the dirent structure as a slice of u64 for word-wise processing
        //use `std::ptr::slice_from_raw_parts` to create a slice from the raw pointer and avoid ubcheck
           // Cast dirent+reclen to u64 slice
        let u64_slice = &*std::ptr::slice_from_raw_parts($dirent as *const u64, reclen_in_u64s);
        //  verify alignment/size
   
        // Calculate position of last word
        // Get the last u64 word in the structure

        let last_word_index = reclen_in_u64s -1; //subtract 1 to get the last index
        // Get the last u64 word from the slice
        // (fine we ensured reclen is a multiple of 8 and reclen_in_u64s is the number of u64 words)
        let last_word_check = u64_slice[last_word_index];



        // Special case: When processing the 3rd u64 word (index 2), we need to mask
        // the non-name bytes (d_type and padding) to avoid false null detection.
        // The 0x00FF_FFFF  mask preserves only the LSB 3 bytes where the name could start.
        let last_word_final = if last_word_index == 2 {
                last_word_check | 0x00FF_FFFF
            } else {
                //what the fuck?     ---love u jc                                 //john carmack
                last_word_check
            };

        // Find null terminator position within the last word (using ideally sse2)
        let remainder_len = 7 - $crate::strlen_asm(last_word_final.to_le_bytes().as_ptr());



         // Calculate true string length:
        // 1. Skip dirent header (8B d_ino + 8B d_off + 2B reclen + 2B d_type) == offset_of!(libc::dirent64,d_name)
        //2. add one to get to the correct index
        //3. Subtract ignored bytes (after null terminator in last word)

        const DIRENT_HEADER_SIZE: usize = std::mem::offset_of!(libc::dirent64,d_name)+1;


        $reclen - DIRENT_HEADER_SIZE - remainder_len

    }};
}

 



///Extremely specific niche use case that i can't be bothered writing up.
/// Constructs a path from the base path and the name pointer in constant time.
/// Variadic for `reclen` (can be passed explicitly or fetched from `dirent`)
#[macro_export]
macro_rules! construct_path_const_time {

    // Single argument version (gets `reclen` from `dirent`)
    ($self:ident, $dirent:expr) => {{
        let reclen = *offset_ptr!($dirent, d_reclen) as usize;
        $crate::construct_path_const_time!($self, $dirent, reclen)
    }};

    // Two argument version (explicit `reclen`)
    ($self:ident, $dirent:expr,$reclen:expr) => {{
        let name_ptr:*const u8 = offset_ptr!($dirent, d_name).cast();
        let name_len = $crate::dirent_const_time_strlen!($dirent,$reclen);
        let name_bytes = &*std::ptr::slice_from_raw_parts(name_ptr, name_len);
        let total_len = $self.base_path_len as usize + name_len;

        std::ptr::copy_nonoverlapping(
            name_bytes.as_ptr(),
            $self
                .path_buffer
                .as_mut_ptr()
                .add($self.base_path_len as usize),
            name_len,
        );

        let full_path = $self.path_buffer.get_unchecked_mut(..total_len);
        full_path
    }};
}

 */

#[macro_export]
/// A macro to extract values from a `libc::dirent64` struct.
/// This macro returns a tuple containing:
/// - A pointer to the name field (null-terminated string) 'd_name' *const u8
/// - The file type 'd_type' as u8 (e.g., DT_REG, DT_DIR)
/// - The inode number 'd_ino' as u64
/// - The record length 'd_reclen' as usize
///  Optionally, a minimal version can be used that excludes the record length.
/// /// Usage:
/// ``` 
/// use libc::dirent64;
/// use crate::get_dirent_vals;
/// let dirent: *const libc::dirent64 = todo!(); // Assume this is a valid pointer to a dirent64 struct
/// let (name_ptr, file_type, inode, reclen) = get_dirent_vals!(dirent);
/// let (name_ptr, file_type, inode) = get_dirent_vals!(@minimal dirent); // Minimal version without reclen
/// ```
/// 
macro_rules! get_dirent_vals {
    ($d:expr) => {{
        // Cast the dirent pointer to a byte pointer for offset calculations
    
        unsafe {
            (
                // d_name: pointer to the name field (null-terminated string)
                $crate::offset_ptr!($d, d_name).cast::<u8>(),
                // d_type: file type (DT_REG, DT_DIR, etc.)
                *$crate::offset_ptr!($d, d_type).cast::<u8>(),
                 // d_ino: inode number
                *$crate::offset_ptr!($d, d_ino) as u64,
                 // d_reclen: record length
                *$crate::offset_ptr!($d, d_reclen) as usize,
                // d_ino: inode number
                
            )
        }
    }};
    (@minimal $d:expr) => {{
        //minimal version, as we don't need reclen for readdir, well we can...if we use my fancy construct_path_const_time! 
        // Cast the dirent pointer to a byte pointer for offset calculations
        unsafe {
            (
                // d_name: pointer to the name field (null-terminated string)
                $crate::offset_ptr!($d, d_name).cast::<u8>(),
                // d_type: file type (DT_REG, DT_DIR, etc.)
                *$crate::offset_ptr!($d, d_type).cast::<u8>(),
                   // d_ino: inode number
                 *$crate::offset_ptr!($d, d_ino) as u64,
            )
        }
    }};
}
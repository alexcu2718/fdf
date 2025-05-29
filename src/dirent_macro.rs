//copied this macro from the standard library
//using it to access offsets in a more strict way, doing this in dirent64 is possible but im considering
//the performance impact of that, this is a bit more readable and less error prone

#![allow(clippy::macro_metavars_in_unsafe)]


#[macro_export]
//#[allow(clippy::ptr_as_ptr)]
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
macro_rules! cstr {
    ($bytes:expr) => {{
        // Debug assert to check test builds for unexpected conditions
        debug_assert!(
            $bytes.len() < $crate::LOCAL_PATH_MAX,
            "Input too large for buffer"
        );

        // Create a PathBuffer
        let mut path_buf = $crate::PathBuffer::new();
        let c_path_buf = path_buf.as_mut_ptr();
        #[allow(unused_unsafe)] //macro collision i cant be bothered to fix rn
        // Copy bytes and null-terminate
        unsafe {
            std::ptr::copy_nonoverlapping($bytes.as_ptr(), c_path_buf, $bytes.len());
            c_path_buf.add($bytes.len()).write(0);
        }

        c_path_buf.cast::<_>()
    }};
}

#[macro_export(local_inner_macros)]
macro_rules! skip_dot_entries {
    ($d_type:expr, $name_ptr:expr $(, $offset:expr, $reclen:expr)?) => {
        //ddd=indicator of whether the dent struct is dir/unknown, if it's unknown, we just need to check the pointer first index, which will eliminate 50%
       #[allow(clippy::macro_metavars_in_unsafe)]//stupid error let me use my hack macros.
        unsafe {
            let ddd = $d_type == libc::DT_DIR || $d_type == libc::DT_UNKNOWN;
            if ddd && *$name_ptr.add(0) == 46 {  // 46 == '.' in ASCII
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

#[macro_export(local_inner_macros)]
macro_rules! process_getdents_loop {
    ($buffer:ident, $fd:ident, $entries:ident, $path_buffer:ident, $path_len:ident, $self:ident) => {
        loop {
            let nread: i64 = unsafe { $buffer.getdents($fd) };

            match nread {
                0 => break, // End of directory
                n if n < 0 => return Err(Error::last_os_error().into()),
                n => {
                    let mut offset = 0;
                    while offset < n as usize {
                        let d: *const libc::dirent64 =
                            unsafe { $buffer.next_getdents_read(offset) };
                        let name_ptr: *const u8 = unsafe { $crate::offset_ptr!(d, d_name).cast() };
                        let d_type: u8 = unsafe { *$crate::offset_ptr!(d, d_type) };
                        let reclen: usize = unsafe { *$crate::offset_ptr!(d, d_reclen) as _ };

                        skip_dot_entries!(d_type, name_ptr, offset, reclen);

                        let name_len = unsafe { $crate::strlen_asm(name_ptr) };
                        let name_bytes =
                            unsafe { &*std::ptr::slice_from_raw_parts(name_ptr, name_len) };
                        let total_len = $path_len + name_len;

                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                name_bytes.as_ptr(),
                                $path_buffer.as_mut_ptr().add($path_len),
                                name_len,
                            );
                        }

                        let full_path = unsafe { $path_buffer.get_unchecked_mut(..total_len) };

                        $entries.push(Self {
                            path: full_path.into(),
                            file_type: $crate::FileType::from_dtype_fallback(d_type, full_path),
                            inode: unsafe { *$crate::offset_ptr!(d, d_ino) },
                            depth: $self.depth + 1,
                            base_len: $path_len as u16,
                        });

                        offset += reclen;
                    }
                }
            }
        }
    };
}

#[macro_export(local_inner_macros)]
macro_rules! init_path_buffer_syscall {
    ($path_buffer:expr, $path_len:ident, $dir_path:expr, $self:expr) => {{
        let buffer_ptr = $path_buffer.as_mut_ptr();
        let needs_slash = $self.depth != 0 || $dir_path != b"/";

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

#[macro_export(local_inner_macros)]
macro_rules! init_path_buffer_readdir {
    ($dir_path:expr, $buffer:expr, $base_len:ident, $needs_slash:ident) => {{
        let dirp = $dir_path.as_bytes();
        let dirp_len = dirp.len();
        $needs_slash = $dir_path.depth != 0 || dirp != b"/";
        $base_len = dirp_len + $needs_slash as usize;

        let buffer_ptr = $buffer.as_mut_ptr();

        unsafe {
            std::ptr::copy_nonoverlapping(dirp.as_ptr(), buffer_ptr, dirp_len);

            if $needs_slash {
                buffer_ptr.add(dirp_len).write(b'/');
            }
        }

        dirp_len
    }};
}

#[macro_export(local_inner_macros)]
macro_rules! copy_name_to_buffer {
    ($self:expr, $name_file:expr, $base_len:expr) => {{
        let name_len = unsafe { $crate::strlen_asm($name_file) };
        let name_bytes: &[u8] = unsafe { &*std::ptr::slice_from_raw_parts($name_file, name_len) };
        let total_path_len = $base_len + name_len;

        unsafe {
            std::ptr::copy_nonoverlapping(
                name_bytes.as_ptr(),
                $self.as_mut_ptr().add($base_len),
                name_len,
            );
        }

        total_path_len
    }};
}


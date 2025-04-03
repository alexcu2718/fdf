//copied this macro from the standard library
//using it to access offsets in a more strict way, doing this in dirent64 is possible but im considering
//the performance impact of that, this is a bit more readable and less error prone

#[macro_export]
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



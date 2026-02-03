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
     // Special case for `d_namlen` - only available on systems that have this field
    ($entry_ptr:expr, d_namlen) => {{
        #[cfg(has_d_namlen)]
        {
            // SAFETY: Caller must ensure pointer is valid
            (*$entry_ptr).d_namlen as usize
        }

        #[cfg(not(has_d_namlen))]
        {
            compile_error!("d_namlen field is not available on this platform - use d_reclen or strlen instead")
        }
    }};


    ($entry_ptr:expr, d_off) => {{
        // SAFETY: Caller must ensure pointer is valid
        (*$entry_ptr).d_off
    }};
    ($entry_ptr:expr,d_name) => {{
         // See reference https://github.com/rust-lang/rust/blob/8712e4567551a2714efa66dac204ec7137bc5605/library/std/src/sys/fs/unix.rs#L740
         (&raw const (*$entry_ptr).d_name).cast::<_>() //we have to have treat  pointer  differently because it's not guaranteed to actually be [0,256] (can't be worked with by value!)
    }};

         ($entry_ptr:expr, d_type) => {{
         #[cfg(not(has_d_type))]
        {
            libc::DT_UNKNOWN // Return D_TYPE unknown on these OS'es, because the struct does not hold the type!
            // https://github.com/rust-lang/rust/blob/d85276b256a8ab18e03b6394b4f7a7b246176db7/library/std/src/sys/fs/unix.rs#L314
        }
        #[cfg(has_d_type)]

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

///A macro to safely access stat entries in a filesystem independent way
// TODO! add other fields as appropriate (this could be pretty long)
// will be public when kinks are worked out
macro_rules! access_stat {
    ($stat_struct:expr, st_mtimensec) => {{
        #[cfg(target_os = "netbsd")]
        {
            $stat_struct.st_mtimensec as _
        } // Why did they do such a specific change

        #[cfg(not(target_os = "netbsd"))]
        {
            $stat_struct.st_mtime_nsec as _
        }
    }};

    ($stat_struct:expr, st_mtime) => {{ $stat_struct.st_mtime as _ }};

    // Inode number, normalised to u64 for compatibility
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

/**
A compile time assert, mirroring `static_assert` from C++

# Examples
```
use fdf::const_assert;
const CONSTANT_VALUE:usize=69;
const_assert!(2 + 2 == 4);
const_assert!(size_of::<u32>() >= 4, "u32 must be 4 bytes!");
const_assert!(CONSTANT_VALUE > 0, "CONSTANT_VALUE must be positive");
```
*/
#[macro_export]
macro_rules! const_assert {
    ($cond:expr $(,)?) => {
        const _: () = {
            if !$cond {
                panic!(concat!("const assertion failed: ", stringify!($cond)));
            }
        };
    };
    ($cond:expr, $($arg:tt)+) => {
        const _: () = {
            if !$cond {
                panic!($($arg)+);
            }
        };
    };
}

/**
 An optimised macro for skipping "." and ".." directory entries

 This macro employs several heuristics to efficiently skip the common "." and ".."
 entries present in every directory. The approach reduces unnecessary work during
 directory traversal and improves CPU branch prediction behaviour.

 Optimisation Strategy:
 1. TYPE FILTERING:
    Since "." and ".." are always directories (or occasionally unknown on unusual
    filesystems), the macro checks `d_type` or `d_namlen` first. This takes advantage
    of the fact that only about ten percent of filesystem entries are directories.
    Consequently:
    - The branch is easier for the CPU to predict
    - Expensive string length or comparison operations are avoided for roughly ninety percent of entries


 Why this matters( a lot of complexity!)
 - These checks are performed for EVERY entry during traversal.
 - Standard traversal code often relies on `strcmp` or `strlen`; this approach
   avoids those calls where possible.
 - Improved branch prediction provides cumulative performance benefits
   across large directory trees.
*/
macro_rules! skip_dot_or_dot_dot_entries {
    ($entry:expr, $action:expr) => {{
        #[allow(unused_unsafe)]
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        /*
        SAFETY: when calling this macro, the pointer has already been ensured to be non-null
        This is internal only because it relies on internal heuristics/guarantees
        */
        unsafe {
            #[cfg(any(
                target_os = "macos",
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "netbsd",
                target_os = "dragonfly",
                target_os="aix",
                target_os="hurd" //even tho i dont plan to support hurd, just better practice.

            ))]
            {
                // BSD/macOS optimisation: check d_namlen first as primary filter
                let namelen = access_dirent!($entry, d_namlen);
                if namelen <= 2 {
                    // Only check d_type for potential "." or ".." entries
                    match access_dirent!($entry, d_type) {
                        libc::DT_DIR | libc::DT_UNKNOWN => {
                            // first 2 bytes, see explanation below
                            let f2b: [u8; 2] = *access_dirent!($entry, d_name);
                            // Combined check using pattern
                            match (namelen, f2b) {
                                (1, [b'.', _]) | (2, [b'.', b'.'])  => $action,
                                _ => (),
                            }
                        }
                        _ => (),
                    }
                }
                // If namelen > 2, skip all checks entirely (covers ~99% of entries)
            }

            #[cfg(any(
                target_os = "linux",
                target_os = "android",
                target_os="fuchsia",
                target_os="redox",
            ))]
            {
                // Linux/Solaris/Illumos/etc optimisation: check d_type first
                const MINIMUM_DIRENT_SIZE: usize =
                    core::mem::offset_of!($crate::dirent64, d_name).next_multiple_of(8);
                $crate::const_assert!(
                    MINIMUM_DIRENT_SIZE == 24,
                    "The minimum dirent size should be 24 on these platforms"
                );

                match access_dirent!($entry, d_type) {
                    libc::DT_DIR | libc::DT_UNKNOWN => {
                        if access_dirent!($entry, d_reclen) == MINIMUM_DIRENT_SIZE {
                            // f3b=first 3 bytes, the d_name is guaranteed to be 5 or more bytes long (from point 19 to 24)
                            // this is because the pointer is padded up to 24, its filled with junk after the first null terminator however.
                            let f3b: [u8; 3] = *access_dirent!($entry, d_name);

                            match f3b {
                                [b'.', 0, _] | [b'.', b'.', 0] => $action, //similar to above
                                _ => (),
                            }
                        }
                    }
                    _ => (),
                }
            }

            #[cfg(not(any(
                target_os = "linux",
                target_os = "macos",
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "dragonfly",
                target_os = "netbsd",
                target_os = "android",
                target_os = "aix",
                target_os="hurd",
                target_os="fuchsia",
                target_os="redox"
            )))]
            {
                // Fallback for other systems: check d_type first
                match access_dirent!($entry, d_type) {
                    libc::DT_DIR | libc::DT_UNKNOWN => {
                        // check above explanation.
                        let f3b: [u8; 3] = *access_dirent!($entry, d_name);
                        match f3b {
                                [b'.', 0, _] | [b'.', b'.', 0] => $action, //similar to above
                                _ => (),
                            }
                    }
                    _ => (),
                }
            }
        }
    }};
}

/**
 Macro to create a const from an env var with compile-time parsing,

 Uses `option_env` under the hood, so it can catch rustc build environment variables.


 (Please read the docs carefully)

 Example usage:
```
// Unfortunately it is *impossible* to test(in rust) this due to build time constants and compile time ordering
 use fdf::const_from_env;

 const_from_env!(MYVAR: usize = "NYVAR", 6969);
 assert!(MYVAR==6969 || MYVAR==5000); //6969 is the default value if the environment variable NYVAR is not set

 const_from_env!(NEG:isize="TEST_VAR",-50);
 assert!(NEG==-50 || NEG==-100); //tested via setting 'export TEST_VAR=-100'.


 const_from_env!(FLOATY:f32="TESTFLOAT",-50.1);
 assert!(FLOATY==-60.1 || FLOATY==-50.1); // tested via setting `export TESTFLOAT=-60.1`

 const_from_env!(TESTDOTFIRST:f64="TESTDOTFIRST",0.00);
 assert!(TESTDOTFIRST==0.01 || TESTDOTFIRST==0.00); // Tested same as above.
```
  This macro allows you to define a constant that can be set via an environment variable at compile time.

 # Notes
 - The value is parsed at compile time
 - Environment variables must contain only numeric and '-'/'+'/'.' characters
 - No scientific characters and not overflow checks are performed due to limitations of const eval.
*/
#[macro_export]
macro_rules! const_from_env {
    ($(#[$meta:meta])* $name:ident: $t:ty = $env:expr, $default:expr) => {
        $(#[$meta])*
        pub const $name: $t = {
            #[allow(clippy::single_call_fn)]
            #[allow(clippy::cast_possible_truncation)] // bad const eval machinery
            #[allow(clippy::cast_sign_loss)] // as above
            #[allow(clippy::indexing_slicing)]
            #[allow(clippy::integer_division_remainder_used)]
            #[allow(clippy::integer_division)] //as above
            #[allow(clippy::missing_asserts_for_indexing)] //compile time only crash(intentional)
            const fn parse_env(s: &str) -> $t {

                let s_bytes = s.as_bytes();
                if s_bytes.len() == 0 {
                    panic!(concat!("Empty environment variable: ", stringify!($env)));
                }

                if !s_bytes.is_ascii(){
                    panic!(concat!("Non ASCII characters in", stringify!($env)));
                }





                const TYPE_OF:&str=stringify!($t);

                const TYPE_OF_AS_BYTES:&[u8]=TYPE_OF.as_bytes();

                $crate::const_assert!(!matches!(TYPE_OF_AS_BYTES,b"f128"),"f128 not tested(due to experimental nature)");
                $crate::const_assert!(!matches!(TYPE_OF_AS_BYTES,b"f16"),"f16 not tested(due to experimental nature)");
                // Eq is not supported in const yet matches is, weird. annoying work around.
                assert!(!(s_bytes[0]==b'-' && TYPE_OF_AS_BYTES[0]==b'u'),concat!("Negative detected in unsigned env var ",stringify!($env)));
                $crate::const_assert!(TYPE_OF_AS_BYTES[0] != b'u' || $default >= <$t>::MIN,concat!("Negative default not allowed for ", stringify!($default)));

                // Detect if we're parsing a float type
                const IS_FLOAT: bool = TYPE_OF_AS_BYTES[0]==b'f';


                let is_negative = s_bytes[0] == b'-';
                let is_positive = s_bytes[0] == b'+';
                let start_idx:usize = if is_negative || is_positive { 1 } else { 0 };

                if IS_FLOAT {

                    const TEN: $t = 10 as $t;
                    const ZERO: $t = 0 as $t;
                    let mut integer_part: $t = ZERO;
                    let mut fraction_part: $t = ZERO;
                    let mut fraction_divisor: $t = 1 as $t;
                    let mut in_fraction = false;
                    let mut i = start_idx;

                    while i < s_bytes.len() {
                        let b = s_bytes[i];
                        match b {
                            b'0'..=b'9' => {
                                let digit = (b - b'0') as $t;
                                if in_fraction {
                                    fraction_divisor *= TEN;
                                    fraction_part = fraction_part * TEN + digit;
                                } else {
                                    integer_part = integer_part * TEN + digit;
                                }
                            }
                            b'.' => {
                                if in_fraction {
                                    panic!(concat!("Multiple decimal points in: ", stringify!($env)));
                                }
                                in_fraction = true;
                            }
                            _ => panic!(concat!("Invalid float value in: ", stringify!($env))),
                        }
                        i += 1;
                    }

                    let mut result = integer_part + (fraction_part / fraction_divisor);
                    if is_negative {
                        result = ZERO -result;
                    }
                    result
                } else {

                    const TEN:$t = 10 as $t;
                    const ZERO:$t = 0 as $t;

                    let mut n = 0 as $t;
                    let mut i = start_idx;

                    while i < s_bytes.len() {
                        let b = s_bytes[i];
                        match b {
                            b'0'..=b'9' => {
                                n = n * TEN + (b - b'0') as $t;
                            }
                            _ => panic!(concat!("Invalid numeric value in: ", stringify!($env))),
                        }
                        i += 1;
                    }

                    if is_negative {
                        n = ZERO - n; // have to do a trick here for signed ints
                    }

                    n
                }
            }

            match option_env!($env) {
                Some(val) => parse_env(val),
                None => $default as _,
            }
        };
    };
}

/// Extremely simple macro for getting rid of boiler blates
macro_rules! return_os_error {
    () => {{
        return Err(std::io::Error::last_os_error().into());
    }};
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
                $fd,
                $path,
                stat_buf.as_mut_ptr(),
                $flags,
            )
        };

        if res == 0 {
            // SAFETY: If the return code is 0, we know the stat structure has been properly initialised
            Ok(unsafe { stat_buf.assume_init() })
        } else {
            Err(std::io::Error::last_os_error().into())
        }
    }};
      // For fstatat with flags - returns FileType directly (kinda like an internal black magic tool for me to save writing so much duplicate code)
     ($syscall:ident, $fd:expr, $path:expr, $flags:expr,DTYPE) => {{
        let mut stat_buf = core::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY:
        // - The path is guaranteed to be null-terminated (CStr)
        let res = unsafe {
            $syscall(
                $fd,
                $path,
                stat_buf.as_mut_ptr(),
                $flags,
            )
        };

        if res == 0 {
            // SAFETY: If the return code is 0, we know it's been initialised properly
            $crate::fs::FileType::from_stat(&unsafe { stat_buf.assume_init() })
        } else {
            $crate::fs::FileType::Unknown
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

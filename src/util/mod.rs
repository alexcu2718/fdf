mod glob;
mod memchr_derivations;
mod printer;
mod utils;

pub use glob::{Error, glob_to_regex};
pub use memchr_derivations::{find_char_in_word, find_last_char_in_word, memrchr};

pub(crate) use printer::write_paths_coloured;
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "emscripten",
    target_os = "redox",
    target_os = "hermit",
    target_os = "fuchsia",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "aix",
    target_os = "hurd"
))]
pub use utils::dirent_const_time_strlen;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub use utils::getdents;
pub(crate) use utils::{BytePath, dirent_name_length};

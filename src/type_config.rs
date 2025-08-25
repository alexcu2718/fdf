use fdf::{BytesStorage, DirEntry, DirEntryFilter};
use std::sync::OnceLock;

static TYPE_FILTER_TYPES: OnceLock<Vec<String>> = OnceLock::new();

//cbf to care about pass by value so im ignoring clippy.
//negligible impact.
//#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::expect_used)]
#[allow(clippy::single_call_fn)]
pub fn build_type_filter<S>(types: &[String]) -> DirEntryFilter<S>
where
    S: BytesStorage + 'static + Clone,
{
    TYPE_FILTER_TYPES.get_or_init(|| types.iter().map(|typ| typ.to_lowercase()).collect());

    // return a function pointer
    filter_by_type
}
#[allow(clippy::single_call_fn)]
fn filter_by_type<S>(entry: &DirEntry<S>) -> bool
where
    S: BytesStorage + 'static + Clone,
{
    // SAFETY: This is safe because `TYPE_FILTER_TYPES` is a `OnceLock` that is
    // initialised exactly once in the `build_type_filter` function. The call to `get()`
    // will succeed, and `unwrap_unchecked()` is safe because we know the value is present.
    let types = unsafe { TYPE_FILTER_TYPES.get().unwrap_unchecked() }; // 

    for type_char in types.iter().flat_map(|cha| cha.chars()) {
        match type_char {
            'd' => {
                if entry.is_dir() {
                    return true;
                }
            }
            'l' => {
                if entry.is_symlink() {
                    return true;
                }
            }
            'f' => {
                if entry.is_regular_file() {
                    return true;
                }
            }
            'p' => {
                if entry.is_pipe() {
                    return true;
                }
            }
            'c' => {
                if entry.is_char_device() {
                    return true;
                }
            }
            'b' => {
                if entry.is_block_device() {
                    return true;
                }
            }
            's' => {
                if entry.is_socket() {
                    return true;
                }
            }
            'e' => {
                if entry.is_empty() {
                    return true;
                }
            }
            'x' => {
                if entry.is_executable() {
                    return true;
                }
            }
            'u' => {
                if entry.is_unknown() {
                    return true;
                }
            }

            _ => {}
        }
    }

    false
}

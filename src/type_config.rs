use fdf::DirEntry;
use std::sync::OnceLock;

static TYPE_FILTER_TYPES: OnceLock<Vec<String>> = OnceLock::new();

#[allow(clippy::needless_pass_by_value)]
pub fn build_type_filter(types: Vec<String>) -> fn(&DirEntry) -> bool {
    TYPE_FILTER_TYPES.get_or_init(|| types.iter().map(|t| t.to_lowercase()).collect());

    // return a function pointer
    filter_by_type
}

fn filter_by_type(entry: &DirEntry) -> bool {
    let types = TYPE_FILTER_TYPES.get().expect("Types not initialised");

    for type_char in types.iter().flat_map(|s| s.chars()) {
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
                if entry.is_fifo() {
                    return true;
                }
            }
            'c' => {
                if entry.is_char() {
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

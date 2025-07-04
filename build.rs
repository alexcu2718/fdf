#![allow(clippy::all)]
#![allow(warnings)]

use std::io::Write;
use std::thread;



use std::env;
use std::fs::File;

use std::collections::HashMap;
use ansic::ansi;
macro_rules! ansi_bytes {
    ($($t:tt)*) => {
        ansi!($($t)*).as_bytes()
    };
}

const COLOUR_RS: &[u8] = ansi_bytes!(rgb(200, 60, 0));
const COLOUR_PY: &[u8] = ansi_bytes!(rgb(0, 200, 200));
const COLOUR_CPP: &[u8] = ansi_bytes!(rgb(0, 100, 200));
const COLOUR_H: &[u8] = ansi_bytes!(rgb(80, 160, 220));
const COLOUR_C: &[u8] = ansi_bytes!(rgb(255, 255, 224));
const COLOUR_LUA: &[u8] = ansi_bytes!(rgb(0, 0, 255));
const COLOUR_HTML: &[u8] = ansi_bytes!(rgb(255, 105, 180));
const COLOUR_CSS: &[u8] = ansi_bytes!(rgb(150, 200, 50));
const COLOUR_JS: &[u8] = ansi_bytes!(rgb(240, 220, 80));
const COLOUR_JSON: &[u8] = ansi_bytes!(rgb(160, 140, 200));
const COLOUR_TOML: &[u8] = ansi_bytes!(rgb(200, 120, 80));
const COLOUR_TXT: &[u8] = ansi_bytes!(rgb(128, 128, 128));
const COLOUR_MD: &[u8] = ansi_bytes!(rgb(100, 180, 100));
const COLOUR_INI: &[u8] = ansi_bytes!(rgb(180, 80, 80));
const COLOUR_CFG: &[u8] = ansi_bytes!(rgb(180, 80, 80));
const COLOUR_XML: &[u8] = ansi_bytes!(rgb(130, 90, 200));
const COLOUR_YML: &[u8] = ansi_bytes!(rgb(130, 90, 200));
const COLOUR_TS: &[u8] = ansi_bytes!(rgb(90, 150, 250));
const COLOUR_SH: &[u8] = ansi_bytes!(rgb(100, 250, 100));
const COLOUR_BAT: &[u8] = ansi_bytes!(rgb(200, 200, 0));
const COLOUR_PS1: &[u8] = ansi_bytes!(rgb(200, 200, 0));
const COLOUR_RB: &[u8] = ansi_bytes!(rgb(200, 0, 200));
const COLOUR_PHP: &[u8] = ansi_bytes!(rgb(80, 80, 200));
const COLOUR_PL: &[u8] = ansi_bytes!(rgb(80, 80, 200));
const COLOUR_R: &[u8] = ansi_bytes!(rgb(0, 180, 0));
const COLOUR_CS: &[u8] = ansi_bytes!(rgb(50, 50, 50));
const COLOUR_JAVA: &[u8] = ansi_bytes!(rgb(150, 50, 50));
const COLOUR_GO: &[u8] = ansi_bytes!(rgb(0, 150, 150));
const COLOUR_SWIFT: &[u8] = ansi_bytes!(rgb(250, 50, 150));
const COLOUR_KT: &[u8] = ansi_bytes!(rgb(50, 150, 250));
const COLOUR_SCSS: &[u8] = ansi_bytes!(rgb(245, 166, 35));
const COLOUR_LESS: &[u8] = ansi_bytes!(rgb(245, 166, 35));
const COLOUR_CSV: &[u8] = ansi_bytes!(rgb(160, 160, 160));
const COLOUR_TSV: &[u8] = ansi_bytes!(rgb(160, 160, 160));
const COLOUR_XLS: &[u8] = ansi_bytes!(rgb(64, 128, 64));
const COLOUR_XLSX: &[u8] = ansi_bytes!(rgb(64, 128, 64));
const COLOUR_SQL: &[u8] = ansi_bytes!(rgb(100, 100, 100));

// Default colors if LS_COLORS is not set
const DEFAULT_SYMLINK_COLOR: &[u8] = ansi_bytes!(rgb(230, 150, 60));
const DEFAULT_DIR_COLOR: &[u8] = ansi_bytes!(rgb(30, 144, 255));

fn main() {

       const MIN_THREADS: usize = 1;
    let num_threads =
        thread::available_parallelism().map_or(MIN_THREADS, core::num::NonZeroUsize::get);

    if num_threads == MIN_THREADS {
        println!("cargo:rustc-env=CPU_COUNT={MIN_THREADS}");
    } else {
        println!("cargo:rustc-env=CPU_COUNT={num_threads}");
    }






    let ls_colors = env::var("LS_COLORS").unwrap_or_default();
    let mut color_map = parse_ls_colors(&ls_colors);

    // Add fallback colors for common extensions
    add_fallback_colors(&mut color_map);

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = std::path::Path::new(&out_dir).join("ls_colors.rs");
    let mut f = File::create(&dest_path).unwrap();
    

    writeln!(f, "use phf::phf_map;").unwrap();
    writeln!(f, "/// This is a compile-time hash map of file extensions to their corresponding ANSI color codes").unwrap();
    writeln!(f, "/// based on the `LS_COLORS` environment variable.").unwrap();
    writeln!(f, "///").unwrap();
    writeln!(f, "/// It provides colour coding for file types in terminal applications.").unwrap();
    writeln!(f, "/// Keys are byte slices representing file extensions.").unwrap();
    writeln!(f, "/// Values are byte slices representing ANSI escape sequences.").unwrap();
    writeln!(f, "/// Generated at build time from the LS_COLORS environment variable.").unwrap();
    writeln!(f, "pub static LS_COLOURS_HASHMAP: phf::Map<&'static [u8], &'static [u8]> = phf_map! {{").unwrap();
    for (ext, escape_seq) in color_map {
        // Convert the escape sequence to bytes and escape special characters
        let escaped_seq = String::from_utf8_lossy(&escape_seq)
            .replace('\\', "\\\\")
            .replace('\"', "\\\"");
        writeln!(
            f,
            "    b\"{}\" => b\"{}\",",
            ext, escaped_seq
        ).unwrap();
    }

    // Add special file types
    writeln!(f, "    b\"symlink\" => b\"{}\",", String::from_utf8_lossy(DEFAULT_SYMLINK_COLOR)).unwrap();
    writeln!(f, "    b\"directory\" => b\"{}\",", String::from_utf8_lossy(DEFAULT_DIR_COLOR)).unwrap();

    writeln!(f, "}};").unwrap();
}

fn parse_ls_colors(ls_colors: &str) -> HashMap<String, Vec<u8>> {
    let mut color_map = HashMap::new();
    
    for entry in ls_colors.split(':') {
        let parts: Vec<&str> = entry.split('=').collect();
        if parts.len() != 2 || !parts[0].starts_with("*.") {
            continue;
        }
        
        let extension = parts[0][2..].to_string();
        if let Some(escape_seq) = get_ansi_escape_sequence(parts[1]) {
            color_map.insert(extension, escape_seq.into_bytes());
        }
    }
    
    color_map
}

fn get_ansi_escape_sequence(code: &str) -> Option<String> {
    let mut bold = false;
    let mut color_code = None;
    
    for part in code.split(';') {
        if let Ok(num) = part.parse::<u8>() {
            match num {
                1 => bold = true,
                30..=37 | 90..=97 => color_code = Some(num),
                _ => {} //tedious to find documents
            }
        }
    }
    
    color_code.map(|code| {
        let mut sequence = String::from("\x1b[");
        if bold {
            sequence.push_str("1;");
        }
        sequence.push_str(&code.to_string());
        sequence.push('m');
        sequence
    })
}

fn add_fallback_colors(color_map: &mut HashMap<String, Vec<u8>>) {
    // Only add fallback if the extension isn't already in the map
    let fallbacks = vec![
        ("rs", COLOUR_RS),
        ("py", COLOUR_PY),
        ("cpp", COLOUR_CPP),
        ("h", COLOUR_H),
        ("c", COLOUR_C),
        ("lua", COLOUR_LUA),
        ("html", COLOUR_HTML),
        ("css", COLOUR_CSS),
        ("js", COLOUR_JS),
        ("json", COLOUR_JSON),
        ("toml", COLOUR_TOML),
        ("txt", COLOUR_TXT),
        ("md", COLOUR_MD),
        ("ini", COLOUR_INI),
        ("cfg", COLOUR_CFG),
        ("xml", COLOUR_XML),
        ("yml", COLOUR_YML),
        ("ts", COLOUR_TS),
        ("sh", COLOUR_SH),
        ("bat", COLOUR_BAT),
        ("ps1", COLOUR_PS1),
        ("rb", COLOUR_RB),
        ("php", COLOUR_PHP),
        ("pl", COLOUR_PL),
        ("r", COLOUR_R),
        ("cs", COLOUR_CS),
        ("java", COLOUR_JAVA),
        ("go", COLOUR_GO),
        ("swift", COLOUR_SWIFT),
        ("kt", COLOUR_KT),
        ("scss", COLOUR_SCSS),
        ("less", COLOUR_LESS),
        ("csv", COLOUR_CSV),
        ("tsv", COLOUR_TSV),
        ("xls", COLOUR_XLS),
        ("xlsx", COLOUR_XLSX),
        ("sql", COLOUR_SQL),
    ];

    for (ext, color) in fallbacks {
        color_map.entry(ext.to_string())
            .or_insert_with(|| color.to_vec());
    }


}
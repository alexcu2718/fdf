use memchr::memrchr;
use std::sync::OnceLock;
use fdf::DirEntry;
use std::io::{stdout, BufWriter, IsTerminal, Write};
use std::collections::HashMap;

const NEWLINE: &[u8] = b"\n";
const RESET: &[u8] = b"\x1b[0m";
const COLOUR_RS: &[u8] = b"\x1b[38;2;200;60;0m";
const COLOUR_PY: &[u8] = b"\x1b[38;2;0;200;200m";
const COLOUR_CPP: &[u8] = b"\x1b[38;2;0;100;200m";
const COLOUR_H: &[u8] = b"\x1b[38;2;80;160;220m";
const COLOUR_C: &[u8] = b"\x1b[38;2;255;255;224m";
const COLOUR_LUA: &[u8] = b"\x1b[38;2;0;0;255m";
const COLOUR_HTML: &[u8] = b"\x1b[38;2;255;105;180m";
const COLOUR_CSS: &[u8] = b"\x1b[38;2;150;200;50m";
const COLOUR_JS: &[u8] = b"\x1b[38;2;240;220;80m";
const COLOUR_JSON: &[u8] = b"\x1b[38;2;160;140;200m";
const COLOUR_TOML: &[u8] = b"\x1b[38;2;200;120;80m";
const COLOUR_TXT: &[u8] = b"\x1b[38;2;128;128;128m";
const COLOUR_MD: &[u8] = b"\x1b[38;2;100;180;100m";
const COLOUR_INI: &[u8] = b"\x1b[38;2;180;80;80m";
const COLOUR_CFG: &[u8] = b"\x1b[38;2;180;80;80m";
const COLOUR_XML: &[u8] = b"\x1b[38;2;130;90;200m";
const COLOUR_YML: &[u8] = b"\x1b[38;2;130;90;200m";
const COLOUR_TS: &[u8] = b"\x1b[38;2;90;150;250m";
const COLOUR_SH: &[u8] = b"\x1b[38;2;100;250;100m";
const COLOUR_BAT: &[u8] = b"\x1b[38;2;200;200;0m";
const COLOUR_PS1: &[u8] = b"\x1b[38;2;200;200;0m";
const COLOUR_RB: &[u8] = b"\x1b[38;2;200;0;200m";
const COLOUR_PHP: &[u8] = b"\x1b[38;2;80;80;200m";
const COLOUR_PL: &[u8] = b"\x1b[38;2;80;80;200m";
const COLOUR_R: &[u8] = b"\x1b[38;2;0;180;0m";
const COLOUR_CS: &[u8] = b"\x1b[38;2;50;50;50m";
const COLOUR_JAVA: &[u8] = b"\x1b[38;2;150;50;50m";
const COLOUR_GO: &[u8] = b"\x1b[38;2;0;150;150m";
const COLOUR_SWIFT: &[u8] = b"\x1b[38;2;250;50;150m";
const COLOUR_KT: &[u8] = b"\x1b[38;2;50;150;250m";
const COLOUR_SCSS: &[u8] = b"\x1b[38;2;245;166;35m";
const COLOUR_LESS: &[u8] = b"\x1b[38;2;245;166;35m";
const COLOUR_CSV: &[u8] = b"\x1b[38;2;160;160;160m";
const COLOUR_TSV: &[u8] = b"\x1b[38;2;160;160;160m";
const COLOUR_XLS: &[u8] = b"\x1b[38;2;64;128;64m";
const COLOUR_XLSX: &[u8] = b"\x1b[38;2;64;128;64m";
const COLOUR_SQL: &[u8] = b"\x1b[38;2;100;100;100m";
// default colors if LS_COLORS is not set
const DEFAULT_SYMLINK_COLOR: &[u8] = b"\x1b[38;2;230;150;60m";
const DEFAULT_DIR_COLOR: &[u8] = b"\x1b[38;2;30;144;255m";

#[allow(clippy::inline_always)]
#[inline(always)]
fn extension_colour( entry: &DirEntry) -> &[u8] {
    // check if it's a symlink and use  LS_COLORS symlink color
    if entry.is_symlink() {
        return SYMLINK_COLOR.get_or_init(|| parse_ls_colors("ln", DEFAULT_SYMLINK_COLOR));
    }
    
    // check if it's a directory and use  LS_COLORS directory color
    if entry.is_dir() {
        return DIR_COLOR.get_or_init(|| parse_ls_colors("di", DEFAULT_DIR_COLOR));
    }
    let bytes=&entry.path;
    // for all other  files, color by extension
    memrchr(b'.', bytes).map_or(RESET, |pos| match &bytes[pos + 1..] {
        b"rs" => COLOUR_RS,
        b"py" => COLOUR_PY,
        b"cpp" => COLOUR_CPP,
        b"h" => COLOUR_H,
        b"c" => COLOUR_C,
        b"lua" => COLOUR_LUA,
        b"html" => COLOUR_HTML,
        b"css" => COLOUR_CSS,
        b"js" => COLOUR_JS,
        b"json" => COLOUR_JSON,
        b"toml" => COLOUR_TOML,
        b"txt" => COLOUR_TXT,
        b"md" => COLOUR_MD,
        b"ini" => COLOUR_INI,
        b"cfg" => COLOUR_CFG,
        b"xml" => COLOUR_XML,
        b"yml" => COLOUR_YML,
        b"ts" => COLOUR_TS,
        b"sh" => COLOUR_SH,
        b"bat" => COLOUR_BAT,
        b"ps1" => COLOUR_PS1,
        b"rb" => COLOUR_RB,
        b"php" => COLOUR_PHP,
        b"pl" => COLOUR_PL,
        b"r" => COLOUR_R,
        b"cs" => COLOUR_CS,
        b"java" => COLOUR_JAVA,
        b"go" => COLOUR_GO,
        b"swift" => COLOUR_SWIFT,
        b"kt" => COLOUR_KT,
        b"scss" => COLOUR_SCSS,
        b"less" => COLOUR_LESS,
        b"csv" => COLOUR_CSV,
        b"tsv" => COLOUR_TSV,
        b"xls" => COLOUR_XLS,
        b"xlsx" => COLOUR_XLSX,
        b"sql" => COLOUR_SQL,
        _ => RESET,
    })
}

#[allow(clippy::inline_always)]
#[inline(always)]
pub fn write_paths_coloured<I>(
    paths: I,
    limit: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>>
where
    I: Iterator<Item = DirEntry>,
{
    let mut buf_writer = BufWriter::new(stdout().lock());
    let use_colors = stdout().is_terminal();

    if use_colors {
        for path in paths.take(limit.unwrap_or(usize::MAX)) {
            buf_writer.write_all(extension_colour( &path))?;
            buf_writer.write_all(&path.path)?;

            // add a trailing slash for directories
            if path.is_dir() {
                buf_writer.write_all(b"/")?;
            }

            buf_writer.write_all(NEWLINE)?;
            buf_writer.write_all(RESET)?;
        }
    } else {
        for path in paths.take(limit.unwrap_or(usize::MAX)) {
            buf_writer.write_all(&path.path)?;

            // same as above
            if path.is_dir() {
                buf_writer.write_all(b"/")?;
            }

            buf_writer.write_all(NEWLINE)?;
        }
    }
    buf_writer.flush()?;
    Ok(())
}


static SYMLINK_COLOR: OnceLock<Box<[u8]>> = OnceLock::new();

static DIR_COLOR: OnceLock<Box<[u8]>> = OnceLock::new();

/// parse the `LS_COLORS` environment variable and get color for a specific key
fn parse_ls_colors(key: &str, default_color: &[u8]) -> Box<[u8]> {
    if let Ok(ls_colors) = std::env::var("LS_COLORS") {
        //  parse the LS_COLORS string into key-value pairs
        let color_map: HashMap<&str, &str> = ls_colors
            .split(':')
            .filter_map(|entry| {
                let parts: Vec<&str> = entry.splitn(2, '=').collect();
                if parts.len() == 2 {
                    Some((parts[0], parts[1]))
                } else {
                    None
                }
            })
            .collect();
        
        //  the color for the specified key
        if let Some(color_code) = color_map.get(key) {
            return ls_color_to_ansi_rgb(color_code).into_bytes().into_boxed_slice();
        }
    }
    
    // deault color if LS_COLORS not set or doesn't contain the key

    default_color.to_vec().into_boxed_slice()
}

/// convert the `LS_COLORS` format (e.g., "01;34") to RGB ANSI escape sequence
fn ls_color_to_ansi_rgb(ls_color: &str) -> String {
    //  color if parsing fails
    let mut rgb = (255, 255, 255);
    
    // check if format contains a color code
    if let Some(color_code) = ls_color.split(';').nth(1).and_then(|s| s.parse::<u8>().ok()) {
        // ANSI colors to RGB mapping
        rgb = match color_code {
            30 => (0, 0, 0),        
            31 => (255, 0, 0),      
            32 => (0, 255, 0),     
            33 => (255, 255, 0),    
            34 => (30, 144, 255),   
            35 => (255, 0, 255),    
            36 => (0, 255, 255),    
         
            90 => (128, 128, 128),  
            91 => (255, 100, 100), 
            92 => (100, 255, 100),  
            93 => (255, 255, 100),  
            94 => (100, 100, 255), 
            95 => (255, 100, 255),  
            96 => (100, 255, 255),
      
            _ => (255, 255, 255),   // default
        };
    }
   
    format!("\x1b[38;2;{};{};{}m", rgb.0, rgb.1, rgb.2)
}


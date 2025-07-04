use compile_time_ls_colours::file_type_colour;
use fdf::BytesStorage;
use fdf::{BytePath, DirEntry, FileType, Result};
use std::io::{BufWriter, IsTerminal, Write, stdout};
const NEWLINE: &[u8] = b"\n";
const NEWLINE_CRLF: &[u8] = b"/\n";
const NEWLINE_RESET: &[u8] = b"\x1b[0m\n";
const NEWLINE_CRLF_RESET: &[u8] = b"/\x1b[0m\n";
const RESET: &[u8] = b"\x1b[0m";

#[allow(clippy::inline_always)]
#[inline(always)]
fn extension_colour<S>(entry: &DirEntry<S>) -> &[u8]
where
    S: BytesStorage + 'static + Clone,
{
    // check if it's a symlink and use  LS_COLORS symlink color
    match entry.file_type() {
        // Handle symlinks first (they override directory status)
        FileType::Symlink => file_type_colour!(symlink),
        FileType::Directory => file_type_colour!(directory),
        FileType::BlockDevice => file_type_colour!(block_device),
        FileType::CharDevice => file_type_colour!(character_device),
        FileType::Socket => file_type_colour!(socket),
        FileType::Pipe => file_type_colour!(pipe),
        //executable isn't here because it requires a stat call, i might add it. doesnt affect performance since printing is the bottleneck

        // for all other  files, color by extension
        _ => entry
            .extension()
            .map_or(RESET, |pos| file_type_colour!(pos)),
    }
}

#[allow(clippy::inline_always)]
#[inline(always)]
pub fn write_paths_coloured<I, S>(paths: I, limit: Option<usize>) -> Result<()>
where
    I: Iterator<Item = Vec<DirEntry<S>>>,
    S: BytesStorage + 'static + Clone,
{
    let std_out = stdout();
    let use_colors = std_out.is_terminal();

    let mut writer = BufWriter::new(std_out.lock());

    let limit_opt: usize = limit.unwrap_or(usize::MAX);

    let check_std_colours = /*arbitrary feature request  */
        std::env::var("FDF_NO_COLOR").is_ok_and(|x| x.eq_ignore_ascii_case("TRUE"));

    if use_colors && !check_std_colours {
        for path in paths.flatten().take(limit_opt) {
            writer.write_all(extension_colour(&path))?;
            writer.write_all(&path)?;
            // add a trailing slash+newline for directories
            if path.is_dir() {
                writer.write_all(NEWLINE_CRLF_RESET)?;
            }
            // add a trailing newline for files
            else {
                writer.write_all(NEWLINE_RESET)?;
            }
        }
    } else {
        for path in paths.flatten().take(limit_opt) {
            writer.write_all(&path)?;
            // add a trailing slash+newline for directories
            if path.is_dir() {
                writer.write_all(NEWLINE_CRLF)?;
            }
            // add a trailing newline for files
            else {
                writer.write_all(NEWLINE)?;
            }
        }
    }
    writer.flush()?;

    Ok(())
}

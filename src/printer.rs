<<<<<<< Updated upstream
=======
#![expect(
    clippy::cast_lossless,
    reason = "casting a bool to a usize is trivially fine here."
)]
use crate::{BytePath as _, DirEntry, FileType, SearchConfigError};
>>>>>>> Stashed changes
use compile_time_ls_colours::file_type_colour;

use crate::{BytePath as _, BytesStorage, DirEntry, FileType, SearchConfigError};
use std::io::{BufWriter, IsTerminal as _, Write as _, stdout};
const NEWLINE: &[u8] = b"\n";
const NEWLINE_CRLF: &[u8] = b"/\n";
const NEWLINE_RESET: &[u8] = b"\x1b[0m\n";
const NEWLINE_CRLF_RESET: &[u8] = b"/\x1b[0m\n";
const RESET: &[u8] = b"\x1b[0m";

#[inline]
<<<<<<< Updated upstream
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "We're only matching on relevant types"
)]
fn extension_colour<S>(entry: &DirEntry<S>) -> &[u8]
where
    S: BytesStorage + 'static + Clone,
{
    // check if it's a symlink and use  LS_COLORS symlink colour
    match entry.file_type() {
        FileType::Symlink => file_type_colour!(symlink),
=======
fn extension_colour(entry: &DirEntry) -> &[u8] {
    match entry.file_type {
        FileType::RegularFile | FileType::Unknown => entry
            .extension()
            .map_or(RESET, |pos| file_type_colour!(pos)),
>>>>>>> Stashed changes
        FileType::Directory => file_type_colour!(directory),
        FileType::BlockDevice => file_type_colour!(block_device),
        FileType::CharDevice => file_type_colour!(character_device),
        FileType::Socket => file_type_colour!(socket),
        FileType::Pipe => file_type_colour!(pipe),
        //executable isn't here because it requires a stat call, i might add it. doesnt affect performance since printing is the bottleneck

<<<<<<< Updated upstream
        // for all other  files, colour by extension
        _ => entry
            .extension()
            .map_or(RESET, |pos| file_type_colour!(pos)),
    }
=======
#[inline]
/// A convennient function to print results
fn write_nocolour<W, I>(writer: &mut W, iter_paths: I) -> std::io::Result<()>
where
    W: std::io::Write,
    I: IntoIterator<Item = DirEntry>,
{
    for path in iter_paths {
        writer.write_all(&path)?;
        // SAFETY: We're indexing in bounds (trivially, either 0 or 1, this should never need to be checked but im too lazy to check assembly on it)
        // If it's a directory, we access index 1, which adds a / to the end
        // Due to this being a difficult to predict branch, it seemed prudent to get rid of.
        writer.write_all(unsafe { NEWLINES_PLAIN.get_unchecked(path.is_dir() as usize) })?;
    }
    Ok(())
}
#[inline]
fn write_coloured<W, I>(writer: &mut W, iter_paths: I) -> std::io::Result<()>
where
    W: std::io::Write,
    I: IntoIterator<Item = DirEntry>,
{
    for path in iter_paths {
        writer.write_all(extension_colour(&path))?;
        writer.write_all(&path)?;
        // SAFETY: as above
        writer.write_all(unsafe { NEWLINES_RESET.get_unchecked(path.is_dir() as usize) })?;
    }
    Ok(())
>>>>>>> Stashed changes
}

#[inline]
pub fn write_paths_coloured<I>(
    paths: I,
    limit: Option<usize>,
    nocolour: bool,
) -> Result<(), SearchConfigError>
where
<<<<<<< Updated upstream
    I: Iterator<Item = Vec<DirEntry<S>>>,
    S: BytesStorage + 'static + Clone,
=======
    I: Iterator<Item = Vec<DirEntry>>,
>>>>>>> Stashed changes
{
    let std_out = stdout();
    let use_colours = std_out.is_terminal();

    let mut writer = BufWriter::new(std_out.lock());

    let limit_opt: usize = limit.unwrap_or(usize::MAX);

    let check_std_colours = nocolour || /*arbitrary feature request  */
        std::env::var("FDF_NO_COLOR").is_ok_and(|x| x.eq_ignore_ascii_case("TRUE"));

    if use_colours && !check_std_colours {
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

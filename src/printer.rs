use crate::{BytePath as _, BytesStorage, DirEntry, FileType, SearchConfigError};
use compile_time_ls_colours::file_type_colour;
use rayon::prelude::*;
use std::io::{BufWriter, IsTerminal as _, Write as _, stdout};
const NEWLINE: &[u8] = b"\n";
const NEWLINE_CRLF: &[u8] = b"/\n";
const NEWLINE_RESET: &[u8] = b"\x1b[0m\n";
const NEWLINE_CRLF_RESET: &[u8] = b"/\x1b[0m\n";
const RESET: &[u8] = b"\x1b[0m";

#[inline]
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "We're only matching on relevant types"
)]
fn extension_colour<S>(entry: &DirEntry<S>) -> &[u8]
where
    S: BytesStorage + 'static + Clone,
{
    // check if it's a symlink and use  LS_COLORS symlink colour
    match entry.file_type {
        FileType::Symlink => file_type_colour!(symlink),
        FileType::Directory => file_type_colour!(directory),
        FileType::BlockDevice => file_type_colour!(block_device),
        FileType::CharDevice => file_type_colour!(character_device),
        FileType::Socket => file_type_colour!(socket),
        FileType::Pipe => file_type_colour!(pipe),
        //executable isn't here because it requires a stat call, i might add it. doesnt affect performance since printing is the bottleneck

        // for all other  files, colour by extension
        _ => entry
            .extension()
            .map_or(RESET, |pos| file_type_colour!(pos)),
    }
}

#[inline]
pub fn write_paths_coloured<I, S>(
    paths: I,
    limit: Option<usize>,
    nocolour: bool,
    sort: bool,
) -> Result<(), SearchConfigError>
where
    I: Iterator<Item = Vec<DirEntry<S>>>,
    S: BytesStorage + 'static + Clone + Send,
{
    let std_out = stdout();
    let use_colours = std_out.is_terminal();
    let mut writer = BufWriter::new(std_out.lock());
    let true_limit = limit.unwrap_or(usize::MAX);

    let check_std_colours =
        nocolour || std::env::var("FDF_NO_COLOR").is_ok_and(|x| x.eq_ignore_ascii_case("TRUE"));
    let use_color = use_colours && !check_std_colours;
    //sorting is a very computationally expensive operation alas because it requires alot of operations!
    // Included for completeness (I will probably need to rewrite all of this eventually)
    if sort {
        let mut collected: Vec<_> = paths.flatten().collect();
        collected.par_sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

        let iter_paths = collected.iter().take(true_limit);

        if use_color {
            for path in iter_paths {
                writer.write_all(extension_colour(path))?;
                writer.write_all(&path)?;
                writer.write_all(if path.is_dir() {
                    NEWLINE_CRLF_RESET
                } else {
                    NEWLINE_RESET
                })?;
            }
        } else {
            for path in iter_paths {
                writer.write_all(&path)?;
                writer.write_all(if path.is_dir() { NEWLINE_CRLF } else { NEWLINE })?;
            }
        }
    } else {
        let iter_paths = paths.flatten().take(true_limit);

        if use_color {
            for path in iter_paths {
                writer.write_all(extension_colour(&path))?;
                writer.write_all(&path)?;
                writer.write_all(if path.is_dir() {
                    NEWLINE_CRLF_RESET
                } else {
                    NEWLINE_RESET
                })?;
            }
        } else {
            for path in iter_paths {
                writer.write_all(&path)?;
                writer.write_all(if path.is_dir() { NEWLINE_CRLF } else { NEWLINE })?;
            }
        }
    }

    writer.flush()?;
    Ok(())
}

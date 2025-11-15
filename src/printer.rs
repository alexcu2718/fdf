#![expect(
    clippy::cast_lossless,
    reason = "casting a bool to a usize is trivially fine here."
)]
use crate::{BytePath as _, DirEntry, FileType, SearchConfigError};
use compile_time_ls_colours::file_type_colour;
use rayon::prelude::*;
use std::io::{BufWriter, IsTerminal as _, Write as _, stdout};
const NEWLINE: &[u8] = b"\n";
const NEWLINE_CRLF: &[u8] = b"/\n";
const NEWLINE_RESET: &[u8] = b"\x1b[0m\n";
const NEWLINE_CRLF_RESET: &[u8] = b"/\x1b[0m\n";
// Creating two look  arrays(look up tables) to do branchless formatting for paths
const NEWLINES_RESET: [&[u8]; 2] = [NEWLINE_RESET, NEWLINE_CRLF_RESET];
const NEWLINES_PLAIN: [&[u8]; 2] = [NEWLINE, NEWLINE_CRLF];

const RESET: &[u8] = b"\x1b[0m";
#[inline]
fn extension_colour(entry: &DirEntry) -> &[u8] {
    match entry.file_type {
        FileType::RegularFile | FileType::Unknown => entry
            .extension()
            .map_or(RESET, |pos| file_type_colour!(pos)),
        FileType::Directory => file_type_colour!(directory),
        FileType::Symlink => {
            // if it returns true, it's definitely a directory
            if entry.is_traversible_cache.get().is_some_and(|x| x) {
                file_type_colour!(directory)
            } else {
                file_type_colour!(symlink)
            }
        }
        FileType::BlockDevice => file_type_colour!(block_device),
        FileType::CharDevice => file_type_colour!(character_device),
        FileType::Socket => file_type_colour!(socket),
        FileType::Pipe => file_type_colour!(pipe),
    }
}

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
}

#[inline]
pub fn write_paths_coloured<I>(
    paths: I,
    limit: Option<usize>,
    nocolour: bool,
    sort: bool,
) -> Result<(), SearchConfigError>
where
    I: Iterator<Item = DirEntry>,
{
    let std_out = stdout();
    let mut writer = BufWriter::new(std_out.lock());
    let true_limit = limit.unwrap_or(usize::MAX);

    let check_std_colours =
        nocolour || std::env::var("FDF_NO_COLOR").is_ok_and(|x| x.eq_ignore_ascii_case("TRUE"));
    let use_colour = std_out.is_terminal() && !check_std_colours;
    /*
    sorting is a very computationally expensive operation alas because it requires alot of operations!
     Included for completeness (I will probably need to rewrite all of this eventually)
     */

    if sort {
        let mut collected: Vec<_> = paths.collect();
        collected.par_sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

        let iter_paths = collected.into_iter().take(true_limit);
        // I do a lot of branch avoidance here
        // this code could definitely be a lot more concise, sorry!

        if use_colour {
            write_coloured(&mut writer, iter_paths)?
        } else {
            write_nocolour(&mut writer, iter_paths)?;
        }
    } else {
        let iter_paths = paths.take(true_limit);

        if use_colour {
            write_coloured(&mut writer, iter_paths)?
        } else {
            write_nocolour(&mut writer, iter_paths)?;
        }
    }

    writer.flush()?;
    Ok(())
}

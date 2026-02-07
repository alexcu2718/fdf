#![expect(
    clippy::cast_lossless,
    reason = "casting a bool to a usize is trivially fine here."
)]
use crate::{
    SearchConfigError, TraversalError,
    fs::{DirEntry, FileType},
    util::BytePath,
};
use compile_time_ls_colours::file_type_colour;
use rayon::prelude::ParallelSliceMut as _;
use std::{
    io::{BufWriter, IsTerminal as _, Write, stdout},
    sync::{Arc, Mutex},
};
const NEWLINE: &[u8] = b"\n";
const NEWLINE_CRLF: &[u8] = b"/\n";
const NEWLINE_RESET: &[u8] = b"\x1b[0m\n";
const NEWLINE_CRLF_RESET: &[u8] = b"/\x1b[0m\n";

const NULL_TERMINATED_CRLF: &[u8] = b"/\0";
const NULL_TERMINATED_NEWLINE: &[u8] = b"\0";
// Creating lookup  arrays(look up tables) to do branchless formatting for paths
const NEWLINES_RESET: [&[u8]; 2] = [NEWLINE_RESET, NEWLINE_CRLF_RESET];
const NEWLINES_PLAIN: [&[u8]; 2] = [NEWLINE, NEWLINE_CRLF];
const NULL_TERMINATED_PLAIN: [&[u8]; 2] = [NULL_TERMINATED_NEWLINE, NULL_TERMINATED_CRLF];

const RESET: &[u8] = b"\x1b[0m";
#[inline]
fn extension_colour(entry: &DirEntry) -> &[u8] {
    match entry.file_type {
        FileType::RegularFile | FileType::Unknown => {
            BytePath::extension(entry) // Use the trait to do this, since root will never be sent down the iterator
                .map_or(RESET, |pos| file_type_colour!(pos))
        }
        FileType::Directory => file_type_colour!(directory),
        FileType::Symlink => match entry.is_traversible_cache.get() {
            Some(true) => file_type_colour!(directory),
            _ => file_type_colour!(symlink),
        },
        FileType::BlockDevice => file_type_colour!(block_device),
        FileType::CharDevice => file_type_colour!(character_device),
        FileType::Socket => file_type_colour!(socket),
        FileType::Pipe => file_type_colour!(pipe),
    }
}

/// A convenient function to print results
#[inline]
#[allow(clippy::fn_params_excessive_bools)] //convenience
fn write_nocolour<W, I>(writer: &mut W, iter_paths: I, null_terminated: bool) -> std::io::Result<()>
where
    W: Write,
    I: IntoIterator<Item = DirEntry>,
{
    let terminator_array = if null_terminated {
        //  https://tenor.com/en-GB/view/the-terminator-you-are-terminated-youre-fired-arnold-schwarzenegger-gif-22848847
        NULL_TERMINATED_PLAIN
    } else {
        NEWLINES_PLAIN
    };

    for path in iter_paths {
        writer.write_all(&path)?;
        // SAFETY: We're indexing in bounds (trivially, either 0 or 1, this should never need to be checked but im too lazy to check assembly on it)
        // If it's a directory, we access index 1, which adds a / to the end
        // Due to this being a difficult to predict branch, it seemed prudent to get rid of.
        writer.write_all(unsafe { terminator_array.get_unchecked(path.is_dir() as usize) })?;
        // I don't append a slash for symlinks that are directories when not sending to stdout
        // This is to avoid calling stat on symlinks. It seems extremely wasteful.
    }
    Ok(())
}

#[inline]
fn write_coloured<W, I>(writer: &mut W, iter_paths: I) -> std::io::Result<()>
where
    W: Write,
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
#[allow(clippy::print_stderr, reason = "only enabled if requested")]
#[allow(clippy::fn_params_excessive_bools)] //convenience
pub fn write_paths_coloured<I>(
    path_iter: I,
    limit: Option<usize>,
    nocolour: bool,
    sort: bool,
    print_errors: bool,
    null_terminated: bool,
    errors: Option<&Arc<Mutex<Vec<TraversalError>>>>,
) -> Result<(), SearchConfigError>
where
    I: Iterator<Item = DirEntry>,
{
    let std_out = stdout();
    let true_limit = limit.unwrap_or(usize::MAX);

    let is_terminal = std_out.is_terminal();

    let check_std_colours = nocolour
        || std::env::var("NO_COLOUR").is_ok_and(|x| x.eq_ignore_ascii_case("TRUE"))
        || std::env::var("NO_COLOR").is_ok_and(|x| x.eq_ignore_ascii_case("TRUE"));
    // TODO document this.

    let use_colour = is_terminal && !check_std_colours;

    #[cfg(not(target_os = "macos"))]
    let mut writer = BufWriter::new(std_out);
    #[cfg(target_os = "macos")]
    let mut writer = if is_terminal {
        BufWriter::new(std_out) //Decrease write syscalls if not sending to stdout. Oddly this doesnt happen on Linux?
    //When profiling via dtruss/dtrace, I found A LOT more write syscalls when sending to non-terminal
    } else {
        BufWriter::with_capacity(32 * 4096, std_out)
    };
    /*
    sorting is a very computationally expensive operation alas because it requires alot of operations!
     Included for completeness (I will probably need to rewrite all of this eventually)
     */

    if sort {
        let mut collected: Vec<_> = path_iter.collect();
        collected.par_sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

        let iter_paths = collected.into_iter().take(true_limit);
        // I do a lot of branch avoidance here
        // this code could definitely be a lot more concise, sorry!

        if use_colour {
            write_coloured(&mut writer, iter_paths)?
        } else {
            write_nocolour(&mut writer, iter_paths, null_terminated)?;
        }
    } else {
        let iter_paths = path_iter.take(true_limit);

        if use_colour {
            write_coloured(&mut writer, iter_paths)?
        } else {
            write_nocolour(&mut writer, iter_paths, null_terminated)?;
        }
    }

    writer.flush()?;

    //If errors were sent, show them.
    if print_errors {
        if let Some(errors_arc) = errors {
            if let Ok(error_vec) = errors_arc.lock() {
                for error in error_vec.iter() {
                    eprintln!("{error}");
                }
            }
        }
    }

    Ok(())
}

#![expect(clippy::indexing_slicing, reason = "trivially in bounds")]
#![allow(clippy::missing_inline_in_public_items)]
use crate::{
    SearchConfigError, TraversalError,
    fs::{DirEntry, FileType},
    util::BytePath,
};
use compile_time_ls_colours::file_type_colour;

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

#[allow(clippy::struct_excessive_bools)]
pub struct PrinterBuilder<I>
where
    I: Iterator<Item = DirEntry>,
{
    limit: usize,
    nocolour: bool,
    sort: bool,
    print_errors: bool,
    null_terminated: bool,
    strip_leading_dot_slash: bool,
    errors: Option<Arc<Mutex<Vec<TraversalError>>>>,
    paths: I,
}

impl<I> PrinterBuilder<I>
where
    I: Iterator<Item = DirEntry>,
{
    #[inline]
    pub(crate) const fn new(paths: I) -> Self {
        Self {
            limit: usize::MAX,
            nocolour: false,
            sort: false,
            print_errors: false,
            null_terminated: false,
            strip_leading_dot_slash: false,
            errors: None,
            paths,
        }
    }

    #[must_use]
    /// Limit the values to print to `limit`
    pub const fn limit(mut self, limit: Option<usize>) -> Self {
        self.limit = match limit {
            Some(lim) => lim,
            None => usize::MAX,
        };
        self
    }

    #[must_use]
    /// Print with no colour if enabled (always disabled with "`NO_COLOUR`" or "`NO_COLOR`" environment variables)
    pub const fn nocolour(mut self, nocolour: bool) -> Self {
        self.nocolour = nocolour;
        self
    }

    #[must_use]
    /// Sort results lexicographically
    pub const fn sort(mut self, sort: bool) -> Self {
        self.sort = sort;
        self
    }

    #[must_use]
    /// Print errors(if errors were requested to be collected)
    pub const fn print_errors(mut self, print_errors: bool) -> Self {
        self.print_errors = print_errors;
        self
    }

    #[must_use]
    /// Print results being null terminated(useful for xargs)
    pub const fn null_terminated(mut self, null_terminated: bool) -> Self {
        self.null_terminated = null_terminated;
        self
    }

    #[must_use]
    /// Strip the leading `./` from results when the search root is the current directory
    pub const fn strip_leading_dot_slash(mut self, strip: bool) -> Self {
        self.strip_leading_dot_slash = strip;
        self
    }

    #[must_use]
    pub(crate) fn errors(mut self, errors: Option<Arc<Mutex<Vec<TraversalError>>>>) -> Self {
        self.errors = errors;
        self
    }

    #[inline]
    #[allow(clippy::print_stderr)] //only enabled if requested
    #[allow(clippy::missing_errors_doc)] //write up docs l ater
    /// Print the results
    pub fn print(self) -> Result<(), SearchConfigError> {
        let std_out = stdout();
        let is_terminal = std_out.is_terminal();
        let use_colour = is_terminal && !Self::colour_disabled(self.nocolour);

        let mut writer = if is_terminal {
            BufWriter::new(std_out)
        } else {
            BufWriter::with_capacity(16 * 4096, std_out) //TODO play with these values?
        };

        if self.sort {
            let mut collected: Vec<_> = self.paths.collect();
            // TODO, this algorithm is extremely slow for large collections...
            // I need to parallelise but it's a lot of work for one function, sign.
            collected.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            Self::write_iter(
                &mut writer,
                collected.into_iter().take(self.limit),
                use_colour,
                self.null_terminated,
                self.strip_leading_dot_slash,
            )?;
        } else {
            Self::write_iter(
                &mut writer,
                self.paths.take(self.limit),
                use_colour,
                self.null_terminated,
                self.strip_leading_dot_slash,
            )?;
        }

        writer.flush()?;

        if self.print_errors
            && let Some(errors_arc) = self.errors.as_ref()
            && let Ok(error_vec) = errors_arc.lock()
        {
            for error in error_vec.iter() {
                eprintln!("{error}");
            }
        }

        Ok(())
    }

    fn colour_disabled(nocolour: bool) -> bool {
        nocolour
            || std::env::var("NO_COLOUR").is_ok_and(|x| x.eq_ignore_ascii_case("TRUE"))
            || std::env::var("NO_COLOR").is_ok_and(|x| x.eq_ignore_ascii_case("TRUE"))
        // BECAUSE IM BRITISH
    }

    #[inline]
    fn write_iter<W, J>(
        writer: &mut W,
        iter_paths: J,
        use_colour: bool,
        null_terminated: bool,
        strip_leading_dot_slash: bool,
    ) -> std::io::Result<()>
    where
        W: Write,
        J: IntoIterator<Item = DirEntry>,
    {
        if use_colour {
            write_coloured(writer, iter_paths, strip_leading_dot_slash)
        } else {
            write_nocolour(writer, iter_paths, null_terminated, strip_leading_dot_slash)
        }
    }
}

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
fn write_nocolour<W, I>(
    writer: &mut W,
    iter_paths: I,
    null_terminated: bool,
    strip_leading_dot_slash: bool,
) -> std::io::Result<()>
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
    // Branchless offset: 2 when stripping `./`, 0 otherwise.
    // Every path is guaranteed to start with `./` when the root was `./`.
    let start = usize::from(strip_leading_dot_slash) * 2;

    for path in iter_paths {
        // SAFETY: when strip_leading_dot_slash is true the root was `./`, so every
        // emitted path is guaranteed to start with `./` (len >= 2). When false,
        // start == 0 so we just take the full slice, which is always valid.
        let bytes = unsafe { path.get_unchecked(start..) };
        writer.write_all(bytes)?;
        // We're indexing in bounds (trivially, either 0 or 1, this should never need to be checked but im too lazy to check assembly on it)
        // If it's a directory, we access index 1, which adds a / to the end
        writer.write_all(terminator_array[usize::from(path.is_dir())])?;
        // I don't append a slash for symlinks that are directories when not sending to stdout
        // This is to avoid calling stat on symlinks. It seems extremely wasteful.
    }
    Ok(())
}

#[inline]
fn write_coloured<W, I>(
    writer: &mut W,
    iter_paths: I,
    strip_leading_dot_slash: bool,
) -> std::io::Result<()>
where
    W: Write,
    I: IntoIterator<Item = DirEntry>,
{
    // as aove.
    let start = usize::from(strip_leading_dot_slash) * 2;
    for path in iter_paths {
        // SAFETY: same guarantee as write_nocolour — root was `./` so len >= 2.
        let bytes = unsafe { path.get_unchecked(start..) };
        writer.write_all(extension_colour(&path))?;
        writer.write_all(bytes)?;
        writer.write_all(NEWLINES_RESET[usize::from(path.is_dir())])?;
    }
    Ok(())
}

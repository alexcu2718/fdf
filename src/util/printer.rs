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
const QUOTE: &[u8] = b"\"";
const EMPTY: &[u8] = b"";

const NULL_TERMINATED_NEWLINE: &[u8] = b"\0";
const NULL_TERMINATED_QUOTED_NEWLINE: &[u8] = b"\"\0";

const PREFIXES: [&[u8]; 2] = [EMPTY, QUOTE];
const PLAIN_SUFFIXES: [&[u8]; 4] = [NEWLINE, b"\"\n", b"/\n", b"/\"\n"];
const NULL_SUFFIXES: [&[u8]; 4] = [
    NULL_TERMINATED_NEWLINE,
    NULL_TERMINATED_QUOTED_NEWLINE,
    b"/\0",
    b"/\"\0",
];
const COLOURED_SUFFIXES: [&[u8]; 4] = [
    RESET_NEWLINE,
    RESET_QUOTED_NEWLINE,
    DIR_RESET_NEWLINE,
    DIR_RESET_QUOTED_NEWLINE,
];

const RESET: &[u8] = b"\x1b[0m";
const RESET_NEWLINE: &[u8] = b"\x1b[0m\n";
const RESET_QUOTED_NEWLINE: &[u8] = b"\x1b[0m\"\n";
const DIR_RESET_NEWLINE: &[u8] = b"/\x1b[0m\n";
const DIR_RESET_QUOTED_NEWLINE: &[u8] = b"/\x1b[0m\"\n";

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
    quoted: bool,
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
            quoted: false,
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
    /// Wrap printed file paths in double quotes
    pub const fn quoted(mut self, quoted: bool) -> Self {
        self.quoted = quoted;
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
                self.quoted,
            )?;
        } else {
            Self::write_iter(
                &mut writer,
                self.paths.take(self.limit),
                use_colour,
                self.null_terminated,
                self.strip_leading_dot_slash,
                self.quoted,
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
    #[allow(clippy::fn_params_excessive_bools)] // convenience
    fn write_iter<W, J>(
        writer: &mut W,
        iter_paths: J,
        use_colour: bool,
        null_terminated: bool,
        strip_leading_dot_slash: bool,
        quoted: bool,
    ) -> std::io::Result<()>
    where
        W: Write,
        J: IntoIterator<Item = DirEntry>,
    {
        if use_colour {
            write_coloured(writer, iter_paths, strip_leading_dot_slash, quoted)
        } else {
            write_nocolour(
                writer,
                iter_paths,
                null_terminated,
                strip_leading_dot_slash,
                quoted,
            )
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
    quoted: bool,
) -> std::io::Result<()>
where
    W: Write,
    I: IntoIterator<Item = DirEntry>,
{
    // Branchless offset: 2 when stripping `./`, 0 otherwise.
    // Every path is guaranteed to start with `./` when the root was `./`.
    let start = usize::from(strip_leading_dot_slash) * 2;
    let prefix = PREFIXES[usize::from(quoted)];
    let suffixes = [PLAIN_SUFFIXES, NULL_SUFFIXES][usize::from(null_terminated)];

    for path in iter_paths {
        // SAFETY: when strip_leading_dot_slash is true the root was `./`, so every
        // emitted path is guaranteed to start with `./` (len >= 2). When false,
        // start == 0 so we just take the full slice, which is always valid.
        let bytes = unsafe { path.get_unchecked(start..) };
        writer.write_all(prefix)?;
        writer.write_all(bytes)?;
        writer.write_all(suffixes[(usize::from(path.is_dir()) << 1) | usize::from(quoted)])?;
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
    quoted: bool,
) -> std::io::Result<()>
where
    W: Write,
    I: IntoIterator<Item = DirEntry>,
{
    // as above.
    let start = usize::from(strip_leading_dot_slash) * 2;
    let prefix = PREFIXES[usize::from(quoted)];
    for path in iter_paths {
        // SAFETY: same guarantee as write_nocolour — root was `./` so len >= 2.
        let bytes = unsafe { path.get_unchecked(start..) };
        writer.write_all(prefix)?;
        writer.write_all(extension_colour(&path))?;
        writer.write_all(bytes)?;
        writer.write_all(
            COLOURED_SUFFIXES[(usize::from(path.is_dir()) << 1) | usize::from(quoted)],
        )?;
    }
    Ok(())
}

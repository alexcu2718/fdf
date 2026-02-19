use clap::{ArgAction, CommandFactory as _, Parser, ValueHint, value_parser};
use clap_complete::aot::{Shell, generate};
use core::num::NonZeroUsize;
use fdf::filters::{FileTypeFilterParser, SizeFilterParser, TimeFilterParser};
use fdf::walk::Finder;
use fdf::{SearchConfigError, filters};
use std::env;
use std::ffi::OsString;
use std::io::stdout;

#[cfg(all(
    any(target_os = "linux", target_os = "android", target_os = "macos"),
    not(miri),
    not(debug_assertions),
    feature = "mimalloc",
))]
//miri doesnt support custom allocators
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc; //Please note, don't  use v3 it has weird bugs. I might try snmalloc in future.

#[derive(Parser)]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[allow(clippy::struct_excessive_bools)]
struct Args {
    #[arg(value_name = "PATTERN", help = "Pattern to search for", index = 1)]
    pattern: Option<String>,
    #[arg(
        value_name = "PATH",
        help = format!("Path to search (defaults to current working directory)"),
        value_hint=ValueHint::DirPath,
        required=false,
        index=2
    )]
    directory: Option<OsString>,
    #[arg(
        short = 'H',
        long = "hidden",
        help = "Shows hidden files eg .gitignore or .bashrc, defaults to off"
    )]
    hidden: bool,

    #[arg(
        short = 'S',
        long = "sort",
        help = "Sort the entries alphabetically (this has quite the performance cost)",
        default_value_t = false
    )]
    sort: bool,
    #[arg(
        short = 's',
        long = "case-sensitive",
        default_value_t = true,
        help = "Enable case-sensitive matching, defaults to false"
    )]
    case_insensitive: bool,
    #[arg(
        short = 'e',
        long = "extension",
        help = "filters based on extension, eg --extension .txt or -E txt",
        long_help = "An example command would be `fdf -HI -e  c '^str' / "
    )]
    extension: Option<String>,
    #[arg(
        short = 'j',
        long = "threads",
        help = "Number of threads to use, defaults to available threads available on your computer"
    )]
    thread_num: Option<NonZeroUsize>,
    #[arg(
        short = 'a',
        long = "absolute-path",
        help = "Starts with the directory entered being resolved to full"
    )]
    absolute_path: bool,

    #[arg(
        short = 'L',
        long = "follow",
        default_value_t = false,
        help = "Include symlinks in traversal,defaults to false"
    )]
    follow_symlinks: bool,
    #[arg(
        long = "nocolour",
        alias = "nocolor",
        default_value_t = false,
        help = "Disable colouring output when sending to terminal"
    )]
    no_colour: bool,
    #[arg(
        short = 'g',
        long = "glob",
        required = false,
        default_value_t = false,
        help = "Use a glob pattern,defaults to off"
    )]
    glob: bool,

    #[arg(
        short = 'n',
        long = "max-results",
        help = "Retrieves the first eg 10 results, 'fdf  -n 10 '.cache' /"
    )]
    top_n: Option<usize>,
    #[arg(
        short = 'd',
        long = "depth",
        alias = "max-depth",
        help = "Retrieves only traverse to x depth"
    )]
    depth: Option<u32>,
    #[arg(
    long = "generate",
    action = ArgAction::Set,
    value_parser = value_parser!(Shell),
    help = "Generate shell completions",
    long_help = "
    Generate shell completions for bash/zsh/fish/powershell
    To use: eval \"$(fdf --generate SHELL)\"
    Example:
    # Add to shell config for permanent use
    echo 'eval \"$(fdf --generate zsh)\"' >> ~/.zshrc"
)]
    generate: Option<Shell>,

    #[arg(
        short = 'p',
        long = "full-path",
        required = false,
        default_value_t = false,
        help = "Use a full path for regex matching, default to false"
    )]
    full_path: bool,

    #[arg(
        short = 'F',
        long = "fixed-strings",
        required = false,
        default_value_t = false,
        help = "Use a fixed string not a regex, defaults to false",
        conflicts_with = "glob"
    )]
    fixed_string: bool,

    #[arg(
        long = "show-errors",
        required = false,
        default_value_t = false,
        help = "Show errors when traversing"
    )]
    show_errors: bool,
    #[arg(
        long = "same-file-system",
        alias="one-file-system", //alias for fd for easier use
        required = false,
        default_value_t = false,
        help = "Only traverse the same filesystem as the starting directory"
    )]
    same_file_system: bool,
    #[arg(
        short = '0',
        long = "print0",
        alias = "null-terminated",
        required = false,
        default_value_t = false,
        help = "Makes all output null terminated as opposed to newline terminated, only applies to non-coloured output and redirected(useful for xargs)"
    )]
    print0: bool,
    #[arg(
        short = 'I',
        long = "no-ignore",
        default_value_t = false,
        help = "Do not respect .gitignore rules during traversal"
    )]
    no_ignore: bool,
    /// Filter by file size
    ///
    /// PREFIXES:
    ///   +SIZE    Find files larger than SIZE
    ///   -SIZE    Find files smaller than SIZE
    ///    SIZE     Find files exactly SIZE (default)
    ///
    /// UNITS:
    ///   b        Bytes (default if no unit specified)
    ///   k, kb    Kilobytes (1000 bytes)
    ///   ki, kib  Kibibytes (1024 bytes)
    ///   m, mb    Megabytes (1000^2 bytes)
    ///   mi, mib  Mebibytes (1024^2 bytes)
    ///   g, gb    Gigabytes (1000^3 bytes)
    ///   gi, gib  Gibibytes (1024^3 bytes)
    ///   t, tb    Terabytes (1000^4 bytes)
    ///   ti, tib  Tebibytes (1024^4 bytes)
    ///
    /// EXAMPLES:
    ///   --size 100         Files exactly 100 bytes
    ///   --size +1k         Files larger than 1000 bytes
    ///   --size -10mb       Files smaller than 10 megabytes
    ///   --size +1gi        Files larger than 1 gibibyte
    ///   --size 500ki       Files exactly 500 kibibytes
    #[arg(
    long = "size",
    allow_hyphen_values = true,
    value_name = "SIZE",
    value_parser = SizeFilterParser,
    help = "Filter by file size (supports custom sizes with +/- prefixes)",
    verbatim_doc_comment
)]
    size: Option<filters::SizeFilter>,
    /// Filter by file modification time
    ///
    /// PREFIXES:
    ///   -TIME    Find files modified within the last TIME (newer)
    ///   +TIME    Find files modified more than TIME ago (older)
    ///    TIME    Same as -TIME (default)
    ///
    /// TIME RANGE:
    ///   TIME..TIME   Find files modified between two times
    ///
    /// UNITS:
    ///   s, sec, second, seconds     - Seconds
    ///   m, min, minute, minutes     - Minutes
    ///   h, hour, hours              - Hours
    ///   d, day, days                - Days
    ///   w, week, weeks              - Weeks
    ///   y, year, years              - Years
    ///
    /// EXAMPLES:
    ///   --time -1h        Files modified within the last hour
    ///   --time +2d        Files modified more than 2 days ago
    ///   --time 1d..2h     Files modified between 1 day and 2 hours ago
    ///   --time -30m       Files modified within the last 30 minutes
    #[arg(
    long = "time",
    short = 'T',
    allow_hyphen_values = true,
    value_name = "TIME",
    value_parser = TimeFilterParser,
    help = "Filter by file modification time (supports relative times with +/- prefixes)",
    verbatim_doc_comment
)]
    time: Option<filters::TimeFilter>,

    #[arg(
    short = 't',
    long = "type",
    required = false,
    value_parser = FileTypeFilterParser,
    help = "Filter by file type",
    //long_help = "Filter by file type:\n  d, dir, directory    - Directory\n  u, unknown           - Unknown type\n  l, symlink, link     - Symbolic link\n  f, file, regular     - Regular file\n  p, pipe, fifo        - Pipe/FIFO\n  c, char, chardev     - Character device\n  b, block, blockdev   - Block device\n  s, socket            - Socket\n  e, empty             - Empty file\n  x, exec, executable  - Executable file",

)]
    type_of: Option<filters::FileTypeFilter>,
}

fn main() -> Result<(), SearchConfigError> {
    let args = Args::parse();

    if let Some(generator) = args.generate {
        let mut cmd = Args::command();
        let bin_name = cmd.get_name().to_owned();
        cmd.set_bin_name("fdf");

        generate(generator, &mut cmd, bin_name, &mut stdout());
        return Ok(());
    }

    let path: OsString = args.directory.unwrap_or_else(|| ".".into());

    let finder = Finder::init(&path)
        .pattern(args.pattern.unwrap_or_else(String::new)) //empty string
        .keep_hidden(!args.hidden)
        .case_insensitive(args.case_insensitive)
        .fixed_string(args.fixed_string)
        .canonicalise_root(args.absolute_path)
        .file_name_only(!args.full_path)
        .extension(args.extension.unwrap_or_else(String::new))
        .max_depth(args.depth)
        .follow_symlinks(args.follow_symlinks)
        .filter_by_size(args.size)
        .filter_by_time(args.time)
        .type_filter(args.type_of)
        .collect_errors(args.show_errors)
        .use_glob(args.glob)
        .same_filesystem(args.same_file_system)
        .respect_gitignore(!args.no_ignore)
        .thread_count(args.thread_num)
        .build()?;

    finder
        .build_printer()?
        .limit(args.top_n)
        .null_terminated(args.print0)
        .nocolour(args.no_colour)
        .print_errors(args.show_errors)
        .print()?;

    Ok(())
}

use clap::{ArgAction, CommandFactory as _, Parser, ValueHint, value_parser};
use clap_complete::aot::{Shell, generate};

use fdf::{FileTypeFilter, Finder, LOCAL_PATH_MAX, SearchConfigError, SizeFilter, SlimmerBytes};
use std::env;
use std::ffi::OsString;
use std::io::stdout;
use std::str;

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
        short = 's',
        long = "case-sensitive",
        default_value_t = true,
        help = "Enable case-sensitive matching, defaults to false"
    )]
    case_insensitive: bool,
    #[arg(
        short='e',
        long = "extension",
        help = format!("filters based on extension, eg --extension .txt or -E txt"),
    )]
    extension: Option<String>,
    #[arg(
        short = 'j',
        long = "threads",
        help = "Number of threads to use, defaults to available threads available on your computer"
    )]
    thread_num: Option<usize>,
    #[arg(
        short = 'a',
        long = "absolute-path",
        help = "Show absolute paths of results, defaults to false"
    )]
    absolute_path: bool,

    #[arg(
        short = 'I',
        long = "include-dirs",
        default_value_t = false,
        help = "Include directories, defaults to off"
    )]
    keep_dirs: bool,

    #[arg(
        short = 'L',
        long = "follow",
        default_value_t = false,
        help = "Include symlinks in traversal,defaults to false"
    )]
    follow_symlinks: bool,
    #[arg(
        long = "nocolour",
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
        help = "Retrieves the first eg 10 results, '.cache' / -n 10"
    )]
    top_n: Option<usize>,
    #[arg(
        short = 'd',
        long = "depth",
        alias = "max-depth",
        help = "Retrieves only traverse to x depth"
    )]
    depth: Option<u16>,
    #[arg(
        long = "generate",
        action = ArgAction::Set,
        value_parser = value_parser!(Shell),
        help = "Generate shell completions"
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
        short = 'S',
        allow_hyphen_values = true,
        value_name = "size",
        help = "Filter by size. Examples '10k' (exactly 10KB),'+1M' (>=1MB),'-1GB' (<= 1GB)\n",
        long_help,
        verbatim_doc_comment
    )]
    size: Option<String>,
    /// Filter by file type, eg -d (directory) -f (regular file)
    ///
    /// Available options are:
    /// d: Directory
    /// u: Unknown
    /// l: Symlink
    /// f: Regular File
    /// p: Pipe
    /// c: Char Device
    /// b: Block Device
    /// s: Socket
    /// e: Empty
    /// x: Executable
    #[arg(
        short = 't',
        long = "type",
        required = false,
        help = "Filter by file type, eg -d (directory) -f(regular file)",
        verbatim_doc_comment
    )]
    type_of: Option<String>,
}

#[allow(clippy::exit)] //exiting for cli use
#[expect(clippy::print_stderr, reason = "Similar to above")]
fn main() -> Result<(), SearchConfigError> {
    if LOCAL_PATH_MAX < libc::PATH_MAX as usize {
        eprintln!("We do not expect LOCAL_PATH_MAX to be less than PATH_MAX");
        std::process::exit(1);
    }

    let args = Args::parse();

    if let Some(generator) = args.generate {
        let mut cmd = Args::command();
        let bin_name = cmd.get_name().to_owned();
        cmd.set_bin_name("fdf");

        generate(generator, &mut cmd, bin_name, &mut stdout());
        return Ok(());
    }

    let start_pattern = args
        .pattern
        .as_ref()
        .map_or_else(|| ".".into(), core::clone::Clone::clone);

    if args.depth.is_some_and(|depth| depth == 0) {
        eprintln!("Error: Depth cannot be 0. Exiting.");
        std::process::exit(1);
    }

    let size_of_file = args.size.map(|file_size| {
        match SizeFilter::from_string(&file_size) {
            Ok(filter) => filter,
            Err(err) => {
                //todo! make these errors prettier
                eprintln!(
                    "Error parsing size filter, please check fdf --help '{file_size}': {err}",
                );
                std::process::exit(1);
            }
        }
    });

    let type_filter = args.type_of.map(|type_str| {
        type_str.chars().next().map_or_else(
            || {
                eprintln!("Error: Empty file type argument");
                std::process::exit(1);
            },
            |c| match FileTypeFilter::from_char(c) {
                Ok(filter) => filter,
                Err(err) => {
                    eprintln!("Error parsing file type: {err}");
                    std::process::exit(1);
                }
            },
        )
    });

    let path = args.directory.unwrap_or_else(|| OsString::from("."));
    let finder: Finder<SlimmerBytes> = Finder::init(&path, &start_pattern)
        .keep_hidden(!args.hidden)
        .case_insensitive(args.case_insensitive)
        .keep_dirs(args.keep_dirs)
        .fixed_string(args.fixed_string)
        .canonicalise_root(args.absolute_path)
        .file_name_only(!args.full_path)
        .extension_match(args.extension)
        .max_depth(args.depth)
        .follow_symlinks(args.follow_symlinks)
        .filter_by_size(size_of_file)
        .type_filter(type_filter)
        .show_errors(args.show_errors)
        .use_glob(args.glob)
        .same_filesystem(args.same_file_system)
        .thread_count(args.thread_num)
        .build()?;

    finder.print_results(args.no_colour, args.top_n)?;
    Ok(())
}

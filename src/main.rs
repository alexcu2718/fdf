//i use very strong lints.
//I USE A VERY STRICT CLIPPY TEST, check clippy_test.sh (i will eventually clean these up)
//cargo clippy --all -- -W clippy::all -W clippy::pedantic -W clippy::restriction -W clippy::nursery -D warnings
#![allow(clippy::absolute_paths)]
#![allow(clippy::single_call_fn)] //naturally in a main function youd excpect this
#![allow(clippy::let_underscore_must_use)]
#![allow(clippy::let_underscore_untyped)]
#![allow(clippy::implicit_return)] //this one is dumb
#![allow(clippy::as_underscore)] // this too
#![allow(clippy::missing_docs_in_private_items)]
#![allow(clippy::blanket_clippy_restriction_lints)]
#![allow(clippy::absolute_paths)] //this ones dumb
#![allow(clippy::arbitrary_source_item_ordering)] //stylistic
#![allow(clippy::std_instead_of_alloc)] //this one is stupid
#![allow(clippy::field_scoped_visibility_modifiers)]
#![allow(clippy::pub_with_shorthand)]
#![allow(clippy::allow_attributes)]
#![allow(clippy::allow_attributes_without_reason)]
#![allow(clippy::map_err_ignore)]
#![allow(clippy::question_mark_used)] //dumb...just dumb
#![allow(clippy::semicolon_inside_block)] //dumb/stylistic
#![allow(clippy::must_use_candidate)] //dumb/stylistic
#![allow(clippy::semicolon_outside_block)] //dumb/stylistic
use clap::{ArgAction, CommandFactory as _, Parser, ValueHint, value_parser};
use clap_complete::aot::{Shell, generate};
use fdf::{DirEntryError, Finder, SlimmerBytes, glob_to_regex};
use std::env;
use std::ffi::OsString;
use std::io::stdout;
use std::path::Path;
use std::str;

use fdf::printer::write_paths_coloured;
use fdf::size_filter::SizeFilter;
mod type_config;
use type_config::build_type_filter;

const FILE_TYPES: &str = "d: Directory
u: Unknown
l: Symlink
f: Regular File
p: Pipe
c: Char Device
b: Block Device
s: Socket
e: Empty
x: Executable";

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
        default_value_t = env!("CPU_COUNT").parse::<usize>().unwrap_or(1),
        help = "Number of threads to use, defaults to available threads available on your computer",
    )]
    thread_num: usize,
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
        short = 't',
        long = "type",
        required = false,
        help="Filter by file type, eg -d (directory) -f(regular file)",
        long_help = format!("Select type of files (can use multiple times).\n Available options are:\n{}", FILE_TYPES),
        value_delimiter = ',',
        num_args = 1..,
    )]
    type_of: Option<Vec<String>>,

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
}

#[allow(clippy::exit)]
#[allow(clippy::print_stderr)]
fn main() -> Result<(), DirEntryError> {
    let args = Args::parse();

    rayon::ThreadPoolBuilder::new()
        .num_threads(args.thread_num)
        .build_global()
        .map_err(DirEntryError::RayonError)?;

    let path = resolve_directory(args.directory, args.absolute_path);

    if let Some(generator) = args.generate {
        let mut cmd = Args::command();
        let bin_name = cmd.get_name().to_owned();
        cmd.set_bin_name("fdf");

        generate(generator, &mut cmd, bin_name, &mut stdout());
        return Ok(());
    }

    let start_pattern = args.pattern.as_ref().map_or_else(
        || {
            eprintln!("Error: No pattern provided. Exiting.");
            std::process::exit(1);
        },
        core::clone::Clone::clone,
    );

    let pattern = if args.fixed_string {
        regex::escape(&start_pattern)
    } else {
        process_glob_regex(&start_pattern, args.glob)
    };

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

    let mut finder: Finder<SlimmerBytes> = Finder::init(&path, &pattern)
        .keep_hidden(!args.hidden)
        .case_insensitive(args.case_insensitive)
        .keep_dirs(args.keep_dirs)
        .file_name_only(args.full_path)
        .extension_match(args.extension)
        .max_depth(args.depth)
        .follow_symlinks(args.follow_symlinks)
        .filter_by_size(size_of_file)
        .build()?;

    if let Some(types) = args.type_of {
        let type_filter = build_type_filter(&types);
        finder = finder.with_type_filter(type_filter);
    }

    let _ = write_paths_coloured(finder.traverse()?.iter(), args.top_n, args.no_colour);

    Ok(())
}

#[allow(clippy::must_use_candidate)]
///simple function to resolve the directory to use.
#[allow(clippy::single_call_fn)]
#[allow(clippy::exit)]
#[allow(clippy::print_stderr)] //this is fine because it's CLI only
fn resolve_directory(args_directory: Option<OsString>, canonicalise: bool) -> OsString {
    let dir_to_use = args_directory.unwrap_or_else(generate_start_prefix);
    let path_check = Path::new(&dir_to_use);

    if !path_check.is_dir() {
        eprintln!("{} is not a directory", dir_to_use.to_string_lossy());
        std::process::exit(1);
    }

    if canonicalise {
        match path_check.canonicalize() {
            //stupid yank spelling.
            Ok(canonical_path) => std::path::PathBuf::into_os_string(canonical_path),
            Err(err) => {
                eprintln!(
                    "Failed to canonicalise path {} {}",
                    path_check.to_string_lossy(),
                    err
                );
                std::process::exit(1);
            }
        }
    } else {
        dir_to_use
    }
}
#[allow(clippy::exit)]
#[allow(clippy::print_stderr)]
#[allow(clippy::single_call_fn)]
fn process_glob_regex(pattern: &str, args_glob: bool) -> String {
    if !args_glob {
        return pattern.into();
    }

    glob_to_regex(pattern).unwrap_or_else(|err| {
        eprintln!("This can't be processed as a glob pattern error is  {err}"); //todo! fix these errors
        std::process::exit(1)
    })
}
#[allow(clippy::single_call_fn)]
fn generate_start_prefix() -> OsString {
    env::current_dir()
        .ok()
        .map(std::path::PathBuf::into_os_string)
        .or_else(|| env::var_os("HOME"))
        .unwrap_or_else(|| OsString::from("~"))
}

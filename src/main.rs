use clap::{Parser, ValueHint};
use fdf::{process_glob_regex, resolve_directory, Finder};
use std::ffi::OsString;
use std::str;
const START_PREFIX: &str = "/";
mod printer;
use printer::write_paths_coloured;
mod type_config;
use type_config::build_type_filter;


const CHARS:[char;9] = ['d', 'l', 'f', 'p', 'c', 'b', 's', 'e', 'x'];

#[derive(Parser)]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[allow(clippy::struct_excessive_bools)]
pub struct Args {
    #[arg(value_name = "PATTERN", help = "Pattern to search for", index = 1)]
    pattern: Option<String>,
    #[arg(
        value_name = "PATH",
        help = format!("Path to search (defaults to {START_PREFIX})\nUse -c to do current directory"),
        value_hint=ValueHint::DirPath,
        required=false,
        index=2
    )]
    directory: Option<OsString>,
    #[arg(
        short = 'c',
        long = "current-directory",
        conflicts_with = "directory",
        help = "Uses the current directory to load\n",
        default_value = "false"
    )]
    current_directory: bool,

    #[arg(
        short = 'E',
        long = "extension",
        help = format!("filters based on extension, options are {CHARS:?} \n"),
    )]
    extension: Option<String>,

    #[arg(
        short = 'H',
        long = "hidden",
        help = "Shows hidden files eg .gitignore or .bashrc\n"
    )]
    hidden: bool,
    #[arg(
        short = 's',
        long = "case-sensitive",
        default_value_t = true,
        help = "Enable case-sensitive matching\n"
    )]
    case: bool,
    #[arg(
        short = 'j',
        long = "threads",
        default_value_t = env!("CPU_COUNT").parse::<usize>().unwrap_or(1),
        help = "Number of threads to use, defaults to available threads",
    )]
    thread_num: usize,

    #[arg(
        short = 'I',
        long = "include-dirs",
        default_value_t = false,
        help = "Include directories\n"
    )]
    keep_dirs: bool,
    #[arg(
        short = 'g',
        long = "glob",
        required = false,
        default_value_t = false,
        help = "Use a glob pattern"
    )]
    glob: bool,

    #[arg(
        short = 'd',
        long = "max-depth",
        required = false,
        help = "Retrieves the first eg 10 results, rlib rs$ -d 10"
    )]
    top_n: Option<usize>,
    #[arg(
        short = 't',
        long = "type",
        required = false,
        help = "Select type of files (can use multiple times)",
        value_delimiter = ',',
        num_args = 1..,
    )]
    type_of: Option<Vec<String>>,

    #[arg(
        short = 'p',
        long = "full-path",
        required = false,
        default_value_t = false,
        help = "Use a full path for regex matching"
    )]
    full_path: bool,

    #[arg(
        short = 'F',
        long = "fixed-strings",
        required = false,
        default_value_t = false,
        help = "Use a fixed string not a regex"
    )]
    fixed_string: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let path = resolve_directory(args.current_directory, args.directory);

    rayon::ThreadPoolBuilder::new()
        .num_threads(args.thread_num)
        .build_global()?;

    let pattern = process_glob_regex(&args.pattern.unwrap_or_else(|| ".".into()), args.glob);

    let pattern = if args.fixed_string {
        regex::escape(&pattern)
    } else {
        pattern
    };

    let extension_match = if args.extension.is_some() {
        Some(args.extension.unwrap().into_bytes().into_boxed_slice())
    } else {
        None
    };

    let keep_dirs = args.keep_dirs;
    let case_insensitive = args.case;
    let hide_hidden = !args.hidden;
    let file_name = args.full_path;
    let mut finder = Finder::new(
        path,
        &pattern,
        hide_hidden,
        case_insensitive,
        keep_dirs,
        file_name,
        extension_match,
    ); 
  // eprintln!("{}",args.full_path);


    if let Some(types) = args.type_of {
        let type_filter = build_type_filter(types);
        finder = finder.with_filter(type_filter);
    }

    let results = finder.traverse().into_iter().skip(1);


    write_paths_coloured(results, args.top_n)?;

    Ok(())
}

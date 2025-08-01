#![allow(clippy::single_call_fn)]
#![allow(clippy::all)]
#![allow(clippy::absolute_paths)]
#![allow(clippy::print_stderr)]
#![allow(clippy::implicit_return)]
#![allow(clippy::str_to_string)]
#![allow(clippy::single_call_fn)]
#![allow(clippy::let_underscore_must_use)]
#![allow(clippy::let_underscore_untyped)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::macro_metavars_in_unsafe)]
#![allow(clippy::shadow_unrelated)]
#![allow(clippy::print_stderr)]
#![allow(clippy::implicit_return)]
#![allow(clippy::as_underscore)]
#![allow(clippy::print_stderr)]
#![allow(clippy::min_ident_chars)]
#![allow(clippy::implicit_return)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(clippy::undocumented_unsafe_blocks)]
#![allow(clippy::blanket_clippy_restriction_lints)]
#![allow(clippy::absolute_paths)]
#![allow(clippy::impl_trait_in_params)]
#![allow(clippy::arbitrary_source_item_ordering)]
#![allow(clippy::std_instead_of_core)]
#![allow(clippy::filetype_is_file)]
#![allow(clippy::missing_assert_message)]
#![allow(clippy::unused_trait_names)]
#![allow(clippy::exhaustive_enums)]
#![allow(clippy::exhaustive_structs)]
#![allow(clippy::missing_inline_in_public_items)]
#![allow(clippy::std_instead_of_alloc)]
#![allow(clippy::unseparated_literal_suffix)]
#![allow(clippy::pub_use)]
#![allow(clippy::field_scoped_visibility_modifiers)]
#![allow(clippy::pub_with_shorthand)]
#![allow(clippy::redundant_pub_crate)]
#![allow(clippy::allow_attributes)]
#![allow(clippy::allow_attributes_without_reason)]
#![allow(clippy::use_debug)]
#![allow(clippy::map_err_ignore)]
#![allow(clippy::exit)]
#![allow(clippy::cast_ptr_alignment)]
#![allow(clippy::multiple_unsafe_ops_per_block)]
#![allow(clippy::pattern_type_mismatch)]
#![allow(clippy::arithmetic_side_effects)]
#![allow(clippy::as_conversions)]
#![allow(clippy::question_mark_used)]
#![allow(clippy::semicolon_if_nothing_returned)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::missing_trait_methods)]
#![allow(clippy::default_numeric_fallback)]
#![allow(clippy::wildcard_enum_match_arm)]
#![allow(clippy::semicolon_inside_block)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::semicolon_outside_block)]
#![allow(clippy::return_and_then)]
#![allow(clippy::cast_possible_wrap)]

use clap::{ArgAction, CommandFactory, Parser, ValueHint, value_parser};
use clap_complete::aot::{Shell, generate};
use fdf::{DirEntryError, Finder, SlimmerBytes, glob_to_regex};
use std::env;
use std::ffi::OsString;
use std::io::stdout;
use std::path::Path;
use std::str;

mod printer;
use printer::write_paths_coloured;
mod type_config;
use type_config::build_type_filter;

//mirroring option in fd but adding unknown as well.
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
        help = format!("Path to search (defaults to current working directory )\n"),
        value_hint=ValueHint::DirPath,
        required=false,
        index=2
    )]
    directory: Option<OsString>,
    #[arg(
        short = 'E',
        long = "extension",
        help = format!("filters based on extension, eg -E .txt or -E txt\n"),
    )]
    extension: Option<String>,

    #[arg(
        short = 'H',
        long = "hidden",
        help = "Shows hidden files eg .gitignore or .bashrc, defaults to off\n"
    )]
    hidden: bool,
    #[arg(
        short = 's',
        long = "case-sensitive",
        default_value_t = true,
        help = "Enable case-sensitive matching, defaults to false\n"
    )]
    case_insensitive: bool,
    #[arg(
        short = 'j',
        long = "threads",
        default_value_t = env!("CPU_COUNT").parse::<usize>().unwrap_or(1),
        help = "Number of threads to use, defaults to available threads\n",
    )]
    thread_num: usize,
    #[arg(
        short = 'a',
        long = "absolute-path",
        help = "Show absolute paths of results, defaults to false\n"
    )]
    absolute_path: bool,

    #[arg(
        short = 'I',
        long = "include-dirs",
        default_value_t = false,
        help = "Include directories, defaults to off\n"
    )]
    keep_dirs: bool,

    #[arg(
        short = 'L',
        long = "follow",
        default_value_t = false,
        help = "Include symlinks in traversal,defaults to false\n"
    )]
    follow_symlinks: bool,
    #[arg(
        short = 'g',
        long = "glob",
        required = false,
        default_value_t = false,
        help = "Use a glob pattern,defaults to off\n"
    )]
    glob: bool,

    #[arg(
        short = 'n',
        long = "max-results",
        help = "Retrieves the first eg 10 results, '.cache' / -n 10\n"
    )]
    top_n: Option<usize>,
    #[arg(
        short = 'd',
        long = "depth",
        help = "Retrieves only traverse to x depth\n"
    )]
    depth: Option<u8>,
    #[arg(
        long = "generate",
        action = ArgAction::Set,
        value_parser = value_parser!(Shell),
        help = "Generate shell completions\n"
    )]
    generate: Option<Shell>,

    #[arg(
        short = 't',
        long = "type",
        required = false,
        help = format!("Select type of files (can use multiple times).\n Available options are:\n{}", FILE_TYPES),
        value_delimiter = ',',
        num_args = 1..,
    )]
    type_of: Option<Vec<String>>,

    #[arg(
        short = 'p',
        long = "full-path",
        required = false,
        default_value_t = false,
        help = "Use a full path for regex matching, default to false\n"
    )]
    full_path: bool,

    #[arg(
        short = 'F',
        long = "fixed-strings",
        required = false,
        default_value_t = false,
        help = "Use a fixed string not a regex, defaults to false\n",
        conflicts_with = "glob"
    )]
    fixed_string: bool,
}

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
        std::clone::Clone::clone,
    );

    let pattern = if args.fixed_string {
        regex::escape(&start_pattern)
    } else {
        process_glob_regex(&start_pattern, args.glob)
    };

    if args.depth.is_some_and(|d| d == 0) {
        eprintln!("Error: Depth cannot be 0. Exiting.");
        std::process::exit(1);
    }

    let mut finder: Finder<SlimmerBytes> = Finder::init(&path, &pattern)
        .keep_hidden(!args.hidden)
        .case_insensitive(args.case_insensitive)
        .keep_dirs(args.keep_dirs)
        .file_name_only(args.full_path)
        .extension_match(args.extension)
        .max_depth(args.depth)
        .follow_symlinks(args.follow_symlinks)
        .build();

    if let Some(types) = args.type_of {
        let type_filter = build_type_filter(&types);
        finder = finder.with_type_filter(type_filter);
    }

    let _ = write_paths_coloured(finder.traverse()?.iter(), args.top_n);

    Ok(())
}

#[allow(clippy::must_use_candidate)]
///simple function to resolve the directory to use.
fn resolve_directory(args_directory: Option<OsString>, canonicalise: bool) -> OsString {
    let dir_to_use = args_directory.unwrap_or_else(|| generate_start_prefix());
    let path_check = Path::new(&dir_to_use);

    if !path_check.is_dir() && !path_check.is_symlink() {
        eprintln!("{} is not a directory", dir_to_use.to_string_lossy());
        std::process::exit(1);
    }

    if canonicalise {
        match path_check.canonicalize() {
            //stupid yank spelling.
            Ok(canonical_path) => canonical_path.into_os_string(),
            Err(e) => {
                eprintln!(
                    "Failed to canonicalise path {} {}",
                    path_check.to_string_lossy(),
                    e
                );
                std::process::exit(1);
            }
        }
    } else {
        dir_to_use
    }
}

fn process_glob_regex(pattern: &str, args_glob: bool) -> String {
    if !args_glob {
        return pattern.into();
    }

    glob_to_regex(pattern).unwrap_or_else(|_| {
        eprintln!("This can't be processed as a glob pattern");
        std::process::exit(1)
    })
}

fn generate_start_prefix() -> OsString {
    env::current_dir()
        .ok()
        .map(|p| p.into_os_string())
        .or_else(|| env::var_os("HOME"))
        .unwrap_or_else(|| OsString::from("."))
}

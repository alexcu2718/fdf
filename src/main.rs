#![feature(core_intrinsics)]
#![allow(internal_features)]
use clap::{value_parser, ArgAction, CommandFactory, Parser, ValueHint};
use clap_complete::aot::{generate, Shell};
use fdf::{process_glob_regex, resolve_directory, Finder};
use std::ffi::OsString;
use std::io::stdout;
use std::str;
const START_PREFIX: &str = "/";
mod printer;
use printer::write_paths_coloured;
mod type_config;
use type_config::build_type_filter;

//mirroring option in fd but adding unknown as well.
const CHARS: [char; 10] = ['d', 'u', 'l', 'f', 'p', 'c', 'b', 's', 'e', 'x'];

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
    case_sensitive: bool,
    #[arg(
        short = 'j',
        long = "threads",
        default_value_t = env!("CPU_COUNT").parse::<usize>().unwrap_or(1),
        help = "Number of threads to use, defaults to available threads",
    )]
    thread_num: usize,
    #[arg(
        short = 'a',
        long = "absolute-path",
       // default_value_t = env!("CPU_COUNT").parse::<usize>().unwrap_or(1),
        help = "Show absolute path",
    )]
    absolute_path: bool,

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
        short = 'n',
        long = "max-results",
        help = "Retrieves the first eg 10 results, rlib rs$ -d 10"
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
        help = "Use a fixed string not a regex",
        conflicts_with = "glob"
        
    )]
    fixed_string: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let path = resolve_directory(args.current_directory, args.directory, args.absolute_path);

    if let Some(generator) = args.generate {
        let mut cmd = Args::command();
        let cmd_clone = cmd.clone();
        generate(
            generator,
            &mut cmd,
            cmd_clone.get_name().to_string(),
            &mut stdout(),
        );
        return Ok(());
    }

    rayon::ThreadPoolBuilder::new()
        .num_threads(args.thread_num)
        .build_global()?;

        let start_pattern = args.pattern.as_ref().map_or_else(|| {
                         eprintln!("Error: No pattern provided. Exiting.");
                             std::process::exit(1);
                        }, std::clone::Clone::clone);


        
    let pattern =if args.fixed_string {regex::escape(&start_pattern)} else{ process_glob_regex(&start_pattern, args.glob)};

    

   

  

    let mut finder = Finder::new(
        path,
        &pattern,
        !args.hidden,
        args.case_sensitive,
        args.keep_dirs,
        args.full_path,
        args.extension.map(|x| x.into_bytes().into()),
        args.depth,
    );

    if let Some(types) = args.type_of {
        let type_filter = build_type_filter(types);
        finder = finder.with_filter(type_filter);
    }

    // let results = finder.traverse().into_iter();

    write_paths_coloured(finder.traverse().iter(), args.top_n)?;

    Ok(())
}

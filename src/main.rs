#![allow(clippy::cognitive_complexity)]
#![allow(clippy::match_bool)]
#![allow(clippy::write_with_newline)]

#![cfg_attr(debug_assertions, allow(dead_code))]
#![cfg_attr(debug_assertions, allow(unused_assignments))]
#![cfg_attr(debug_assertions, allow(unused_mut))]
#![cfg_attr(debug_assertions, allow(unused_variables))]

mod cmd;
mod ctx;
mod gen;
mod platform;

use clap::{Arg, App, SubCommand};
use semver::Version;
use std::error::Error;
use std::{fmt, fmt::{Display}};
use std::path::PathBuf;

fn main() {
  // Initialize.
  let commands   = cmd::init();
  let platforms  = platform::init();
  let generators = gen::init();

  // Parse the environment variables.
  let env: ctx::Env = envy::from_env()
    .check(|| "Failed to parse environment variables");

  // Parse the command line.
  let args = App::new(env!("CARGO_PKG_NAME"))
    .version(env!("CARGO_PKG_VERSION"))
    .author(env!("CARGO_PKG_AUTHORS"))
    .about(env!("CARGO_PKG_DESCRIPTION"))
    .arg(Arg::with_name("FOLDER")
         .help("Input folder containing source files")
         .required(true))
    .arg(Arg::with_name("build")
         .short("b")
         .long("build")
         .value_name("FOLDER")
         .help("Where to store the generated project files")
         .takes_value(true))
    .arg(Arg::with_name("config")
         .short("c")
         .long("config")
         .value_name("FILE")
         .help("Name of the build file")
         .takes_value(true))
    // .arg(Arg::with_name("v") // TODO use this
    //      .short("v")
    //      .multiple(true)
    //      .help("Verbosity level"))
    .subcommands(commands.iter().map(|(name, cmd)| {
      cmd.init(SubCommand::with_name(name))
    }))
    .get_matches();

  let input_dir = PathBuf::from(args.value_of("FOLDER").unwrap())
    .canonicalize()
    .unwrap();
  let build_dir = args.value_of("build")
    .map(PathBuf::from)
    .or_else(|| Some(std::env::current_dir().unwrap()))
    .unwrap()
    .canonicalize().unwrap();

  // Load the project's configuration file.
  let mut bytes = Vec::new();
  let project: ctx::Project = {
    use std::io::Read;
    let path = input_dir.join(args.value_of("config").unwrap_or("Jank.toml"));

    let mut f = std::fs::File::open(&path)
      .check(|| format!("Failed to open config file ({:?})", path));

    f.read_to_end(&mut bytes)
      .check(|| format!("Failed to load config file ({:?})", path));

    toml::from_slice(&bytes)
      .check(|| format!("Failed to read the project file ({:?})", path))
  };

  is_supported(&project.min_janky_version).check(|| "Min version check failed");

  (!project.targets.is_empty()).check(|| "No targets in project configuration");

  // Resolve the project's files.
  let sources   = find_all_files(&input_dir, &project.targets, |x| &x.sources);
  let resources = find_all_files(&input_dir, &project.targets, |x| &x.resources);

  let assets = project.targets.iter()
    .fold(ctx::AllFiles::new(), |mut assets, (name, target)| {
      assets.push(match target.assets {
        None      => Vec::new(),
        Some(dir) => find_files(&input_dir, &[[dir, "/**/*"].join("").as_str()])
          .check(|| format!("Failed to resolve assets for target {}", name))
      });
      assets
    });

  let metafiles = std::fs::read_dir(&input_dir).unwrap()
    .fold(Vec::new(), |mut files, entry| {
      if let Ok(e) = entry {
        // Poor man's gitignore, didn't use gitignore.rs because it is too slow.
        match e.path().to_str().unwrap() {
          ".git" | ".DS_Store" => {},
          _ => if let Ok(meta) = e.metadata() {
            files.push(ctx::FileInfo { path: e.path(), meta });
          }
        }
      }
      files
    });

  // Resolve target references (TODO: should probably check if arch/platform matches)
  let extends = project.targets.values().map(|target| {
    target.extends.iter().map(|target_name| {
      project.targets.keys()
        .position(|name| name == target_name)
        .check(|| format!("No such target to extend: {}", target_name))
    }).collect::<Vec<usize>>()
  }).collect::<ctx::Extends>();

  let extended = project.targets.keys().map(|target_name| {
    project.targets.values().enumerate().map(|(index, target)| {
      match target.extends.contains(target_name) {
        true  => Some(index),
        false => None
      }
    }).flatten()
      .collect::<Vec<usize>>()
  }).collect::<ctx::Extends>();

  // println!("{:#?}", project);

  // Execute the requested command.
  let defaults = ctx::Settings::defaults();
  let ctx = ctx::Context {
    env:       &env,
    args:      &args,
    project:   &project,
    extends:   &extends,
    extended:  &extended,
    sources:   &sources,
    resources: &resources,
    assets:    &assets,
    metafiles: &metafiles,
    profiles:  profile_names(&defaults, &project),
    build_rel: pathdiff::diff_paths(&build_dir, &input_dir).unwrap(),
    input_rel: pathdiff::diff_paths(&input_dir, &build_dir).unwrap(),
    input_dir,
    build_dir,
    defaults,
    commands,
    platforms,
    generators
  };

  let cmd_name = ctx.args.subcommand_name().unwrap_or("gen");
  ctx.commands[cmd_name].run(&ctx)
    .check(|| format!("Failed to run command ({})", cmd_name));
}


// Utilities
// -----------------------------------------------------------------------------------

#[derive(Debug)]
struct MinVerError {
  expected: Version,
  current:  Version
}

impl Display for MinVerError {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "{}: expected {} but running {}",
           self.description(), self.expected, self.current)
  }
}

impl Error for MinVerError {
  fn description(&self) -> &str {
    "Project does not support this version"
  }
}

fn is_supported(min_version: &str) -> ctx::DynResult<()> {
  if !min_version.is_empty() {
    let expected = Version::parse(min_version)?;
    let current  = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
    if expected > current {
      return Err(Box::new(MinVerError { expected, current }))
    }
  }
  Ok(())
}

pub fn profile_names<'a>(profiles: &ctx::Profiles<'a>, project: &ctx::Project<'a>) -> Vec<&'a str> {
  let mut v = profiles.keys().cloned().collect::<Vec<&'a str>>();

  v.extend(project.profiles.keys().cloned());

  for t in project.targets.values() {
    v.extend(t.profiles.keys().cloned());
  }

  v.sort_unstable();
  v.dedup();
  v
}

fn find_all_files<'a, F>(input_dir: &PathBuf,
                         targets: &'a std::collections::HashMap<&str, ctx::Target<'a>>,
                         get_patterns: F) -> ctx::AllFiles where
  F: Fn(&'a ctx::Target<'a>) -> &Vec<&str>
{
  let mut files = ctx::AllFiles::new();
  for (name, target) in targets {
    files.push(find_files(&input_dir, get_patterns(target))
               .check(|| format!("Failed to resolve files for target {}", name)));
  }
  files
}

fn find_files(dir: &PathBuf, patterns: &[&str]) -> ctx::DynResult<ctx::TargetFiles> {
  let mut sep_buf = [0; 2]; // FIXME there has to be a better way
  let sep = std::path::MAIN_SEPARATOR.encode_utf8(&mut sep_buf);

  // FIXME: ugly hack because glob() does not handle windows verbatim paths
  #[cfg(windows)]      let prefix = &dir.to_str().unwrap()[4..];
  #[cfg(not(windows))] let prefix = dir.to_str().unwrap();
  #[cfg(windows)]      let prefix_path = PathBuf::from(prefix);
  #[cfg(not(windows))] let prefix_path = dir;

  let mut files = Vec::new();
  for pattern in patterns {
    #[cfg(windows)]      let fixed_pattern = pattern.replace("/", "\\");
    #[cfg(windows)]      let pattern_str = &fixed_pattern;
    #[cfg(not(windows))] let pattern_str = pattern;

    for m in glob::glob(&[prefix, sep, pattern_str].join(""))? {
      let path = PathBuf::from(m?.strip_prefix(&prefix_path)?);
      let meta = std::fs::metadata(dir.join(&path))?;
      files.push(ctx::FileInfo { path, meta });
    }
  }
  Ok(files)
}


// Dumb error handling
// -----------------------------------------------------------------------------

trait Check {
  type R;
  fn check<F, S>(self, msg: F) -> Self::R where F: FnOnce() -> S, S: Display;
}

impl Check for bool {
  type R = ();
  fn check<F, S>(self, msg: F) where F: FnOnce() -> S, S: Display {
    if !self {
      fatal(msg());
    }
  }
}

impl<T> Check for Option<T> {
  type R = T;
  fn check<F, S>(self, msg: F) -> Self::R where F: FnOnce() -> S, S: Display {
    match self {
      None    => fatal(msg()),
      Some(v) => v
    }
  }
}

impl<T, E> Check for Result<T, E> where E: Display {
  type R = T;
  fn check<F, S>(self, msg: F) -> Self::R where F: FnOnce() -> S, S: Display {
    match self {
      Ok (v) => v,
      Err(e) => fatal(format!("{}: {}", msg(), e))
    }
  }
}

fn fatal<S: Display>(msg: S) -> ! {
  eprintln!("{}", msg);
  std::process::exit(1)
}

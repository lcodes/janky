#![allow(clippy::cognitive_complexity)]
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

  let sources   = find_all_files(&input_dir, &project.targets, |x| &x.sources);
  let resources = find_all_files(&input_dir, &project.targets, |x| &x.resources);

  let mut assets = ctx::AllFiles::new();
  for (name, target) in &project.targets {
    assets.push(match target.assets {
      None      => Vec::new(),
      Some(dir) => find_files(&input_dir, &[[dir, "/**/*"].join("").as_str()])
        .check(|| format!("Failed to resolve assets for target {}", name))
    });
  }

  // #[cfg(debug_assertions)]
  // println!("{:#?}", project);

  // Execute the requested command.
  let ctx = ctx::Context {
    commands,
    platforms,
    generators,
    input_dir,
    build_dir,
    env:       &env,
    args:      &args,
    project:   &project,
    sources:   &sources,
    resources: &resources,
    assets:    &assets,
    profiles:  ctx::Settings::defaults()
  };

  let cmd_name = ctx.args.subcommand_name().unwrap_or("gen");
  ctx.commands[cmd_name].run(&ctx)
    .check(|| format!("Failed to run command ({})", cmd_name));
}

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
  let mut files = Vec::new();
  for pattern in patterns {
    for m in glob::glob(dir.join(pattern).to_str().unwrap())? {
      let path = PathBuf::from(m?.strip_prefix(dir)?);
      let meta = std::fs::metadata(dir.join(&path))?;
      files.push(ctx::FileInfo { path, meta });
    }
  }
  Ok(files)
}

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

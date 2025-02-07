use clap::{App, ArgMatches};
use serde::Deserialize;
use serde_repr::Deserialize_repr;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};


// Lazy Error Handling
// -----------------------------------------------------------------------------

#[derive(Debug)]
pub struct StrError(pub String);

impl std::fmt::Display for StrError {
  fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    write!(f, "{}", self.0)
  }
}

impl std::error::Error for StrError {
  fn description(&self) -> &str {
    self.0.as_str()
  }
}


// Interfaces
// -----------------------------------------------------------------------------

pub trait Command : Sync {
  fn init<'a, 'b>(&self, cmd: App<'a, 'b>) -> App<'a, 'b>;

  fn run(&self, ctx: &Context) -> RunResult;
}

pub trait Platform : Sync {
  fn get_platform_type(&self) -> PlatformType;

  fn supports_architecture(&self, a: Architecture) -> bool;

  fn run(&self, ctx: &Context) -> RunResult;
}

pub trait Generator : Sync {
  fn supports_platform(&self, p: PlatformType) -> bool;

  fn run(&self, ctx: &Context) -> RunResult;
}


// Execution Context
// -----------------------------------------------------------------------------

pub type DynResult<T> = Result<T, Box<dyn std::error::Error>>;
pub type RunResult    = DynResult<()>;

pub type Commands   = BTreeMap<&'static str, Box<dyn Command>>;
pub type Generators = BTreeMap<&'static str, Box<dyn Generator>>;
pub type Platforms  = Vec<Box<dyn Platform>>;

pub type TargetFiles  = Vec<FileInfo>;
pub type AllFiles     = Vec<TargetFiles>;
pub type Profiles<'a> = HashMap<&'a str, Vec<Profile<'a>>>;
pub type Strings<'a>  = Cow<'a, [&'a str]>;
pub type Extends      = Vec<Vec<usize>>;

pub struct Context<'a> {
  pub commands:   Commands,
  pub platforms:  Platforms,
  pub generators: Generators,

  pub input_dir: PathBuf,
  pub build_dir: PathBuf,
  pub build_rel: PathBuf,
  pub input_rel: PathBuf,

  pub env:       &'a Env,            // Environment variables
  pub args:      &'a ArgMatches<'a>, // Command-line arguments
  pub project:   &'a Project<'a>,    // Parsed project definition
  pub extends:   &'a Extends,        // Lists of referencing target indices, by target index
  pub extended:  &'a Extends,        // Inverse of extends
  pub sources:   &'a AllFiles,       // Resolved source files, by target index
  pub resources: &'a AllFiles,       // Resolved resource files, by target index
  pub assets:    &'a AllFiles,       // Resolved asset files, by target index
  pub metafiles: &'a TargetFiles,    // Resolved files at the project's root

  pub profiles: Vec<&'a str>,        // Names for all the build profiles
  pub defaults: Profiles<'a>         // Built-in default settings for profiles
}

impl<'a> Context<'a> {
  pub fn get_target(&self, index: usize) -> &Target<'a> {
    self.project.targets.values().nth(index).unwrap()
  }
}

#[derive(Debug)]
pub struct FileInfo {
  pub path: PathBuf,
  pub meta: std::fs::Metadata
}

impl FileInfo {
  pub fn to_str(&self) -> &'_ str {
    self.path.to_str().unwrap()
  }
  pub fn path(&self) -> &'_ PathBuf {
    &self.path
  }
  pub fn name(&self) -> &'_ str {
    self.path.file_name().unwrap().to_str().unwrap()
  }
  pub fn extension(&self) -> &'_ str {
    self.path.extension().unwrap().to_str().unwrap()
  }

  pub fn is_source_no_objc(&self) -> bool {
    self.meta.is_file() && match self.extension() {
      "m" | "mm" => false,
      _          => true
    }
  }

  pub fn is_header(&self) -> bool {
    self.meta.is_file() && match self.extension() {
      "h" | "hpp" => true,
      _           => false
    }
  }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Env {
  pub cflags:   String,
  pub cxxflags: String,
  pub ldflags:  String,

  pub jank_xcode_team: Option<String>
}


// Project Definitions
// -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project<'a> {
  #[serde(rename = "project")]
  #[serde(borrow)]
  pub info: ProjectInfo<'a>,

  #[serde(default)]
  pub profiles: Profiles<'a>,

  pub targets: HashMap<&'a str, Target<'a>>
}

impl<'a> std::ops::Deref for Project<'a> {
  type Target = ProjectInfo<'a>;

  fn deref(&self) -> &ProjectInfo<'a> {
    &self.info
  }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInfo<'a> {
  pub name:    &'a str,
  pub version: &'a str,

  #[serde(default)]
  pub description: &'a str,

  #[serde(default)]
  pub min_janky_version: &'a str,

  #[serde(flatten)]
  pub filter: TargetFilter,

  #[serde(flatten)]
  pub settings: Settings<'a>,

  #[serde(default)]
  pub visual_studio: VisualStudioSettings,

  #[serde(default)]
  pub xcode: XcodeSettings
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualStudioSettings {

}

impl Default for VisualStudioSettings {
  fn default() -> Self {
    VisualStudioSettings {}
  }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XcodeSettings {
  pub group_by_target: bool
}

impl Default for XcodeSettings {
  fn default() -> Self {
    XcodeSettings {
      group_by_target: true
    }
  }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetFilter {
  #[serde(default)]
  pub platforms: Vec<PlatformType>,

  #[serde(default)]
  pub architectures: Vec<Architecture>
}

impl TargetFilter {
  pub fn matches_platform(&self, p: PlatformType) -> bool {
    self.platforms.is_empty() || self.platforms.contains(&p)
  }

  pub fn matches_architecture(&self, a: Architecture) -> bool {
    self.architectures.is_empty() || self.architectures.contains(&a)
  }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target<'a> {
  #[serde(default)]
  #[serde(rename = "type")]
  pub target_type: TargetType,

  /// Source code files (h, hpp, c, cpp, m, mm, etc)
  #[serde(borrow)]
  pub sources: Vec<&'a str>,

  /// Resource data files (copied to target, platform-independent)
  #[serde(default)]
  pub resources: Vec<&'a str>,

  /// Asset data files (embedded in target, platform-specific rules)
  pub assets: Option<&'a str>,

  #[serde(default)]
  pub depends: Vec<&'a str>,

  #[serde(default)]
  pub extends: Vec<&'a str>,

  #[serde(flatten)]
  pub filter: TargetFilter,

  #[serde(flatten)]
  pub settings: Settings<'a>,

  #[serde(default)]
  pub profiles: Profiles<'a>,

  #[serde(default)]
  pub filters: HashMap<PathBuf, Vec<PlatformType>>
}

impl Target<'_> {
  pub fn match_file(&self, file: &Path, platform: PlatformType) -> bool {
    match self.filters.get(file.parent().unwrap()) {
      None    => true,
      Some(f) => f.contains(&platform)
    }
  }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile<'a> {
  #[serde(default)]
  #[serde(alias = "arch")]
  pub architecture: Architecture,

  #[serde(default)]
  #[serde(rename = "platform")]
  pub platform_type: PlatformType,

  #[serde(borrow)]
  #[serde(flatten)]
  pub settings: Settings<'a>
}

impl<'a> Profile<'a> {
  fn new(settings: Settings<'a>) -> Self {
    Profile {
      architecture:  Architecture::default(),
      platform_type: PlatformType::default(),
      settings
    }
  }

  // pub fn merge(&self, profiles: &'a Profiles<'a>, name: &'a str) -> Self {

  // }
}


// Misc. Types
// -----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[repr(i8)]
pub enum Architecture {
  #[serde(skip)]
  Any   = -1,
  X86   =  0,
  X64   =  1,
  ARM   =  2,
  ARM64 =  3,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[repr(i8)]
pub enum PlatformType {
  #[serde(skip)]
  Any     = -1,
  Windows =  0,
  Linux   =  1,
  MacOS   =  2,
  IOS     =  3,
  TVOS    =  4,
  WatchOS =  5,
  Android =  6,
  HTML5   =  7
}

impl PlatformType {
  pub fn to_str(self) -> &'static str {
    match self {
      Self::Any => unreachable!(),
      Self::Windows => "Windows",
      Self::Linux   => "linux",
      Self::MacOS   => "macOS",
      Self::IOS     => "iOS",
      Self::TVOS    => "tvOS",
      Self::WatchOS => "watchOS",
      Self::Android => "Android",
      Self::HTML5   => "HTML5"
    }
  }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub enum TargetType {
  /// Automatically detect the target type based on source file names.
  #[serde(skip)]
  Auto,
  /// Doesn't participate in any build. Used to contain files only.
  None,
  /// Using custom build commands.
  Custom,
  /// A command-line application.
  Console,
  /// A windowed application. Only different from Console on macOS and Windows.
  Application,
  /// A static library, generates a *.lib or *.a file.
  StaticLibrary,
  /// A dynamic library, generates a *.dll, *.so or *.dylib file.
  SharedLibrary
}

impl Default for Architecture {
  fn default() -> Self { Architecture::Any }
}
impl Default for PlatformType {
  fn default() -> Self { PlatformType::Any }
}
impl Default for TargetType {
  fn default() -> Self { TargetType::Auto }
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub enum Optimize {
  None,
  Size,
  Speed,
  Full
}

#[derive(Clone, Copy, Debug, Deserialize_repr)]
#[repr(u8)]
pub enum CStandard {
  C89 = 89,
  C99 = 99,
  C11 = 11
}

#[derive(Clone, Copy, Debug, Deserialize_repr)]
#[repr(u8)]
pub enum CXXStandard {
  CXX03 =  3,
  CXX11 = 11,
  CXX14 = 14,
  CXX17 = 17
}


// Build Settings
// -----------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct Settings<'a> {
  // General
  // - toolset (msvc, clang, gcc ; version)

  // Compiler
  #[serde(borrow)]
  pub include_dirs: Strings<'a>,
  // - debug symbols

  pub warning_level: Option<u8>,
  pub warning_as_error: Option<bool>,

  // Optimizations
  pub optimize: Option<Optimize>,
  pub strict_aliasing: Option<bool>,
  pub omit_frame_pointer: Option<bool>,

  // Preprocessor
  pub defines: Strings<'a>,
  pub undefs: Strings<'a>,

  // Codegen
  pub enable_exceptions: Option<bool>,
  // - simd (neon, sse, avx, ...)
  // - FP abi (soft, softFP, hard)
  // - PIC

  // Language
  pub enable_rtti: Option<bool>,
  pub c_standard: Option<CStandard>,
  pub cxx_standard: Option<CXXStandard>,
  // - stdlib: static/shared, debug/release, msvc/llvm/gcc/stlport/runtime

  // PCH
  // - Enable, file, build file

  // Linker
  pub link_incremental: Option<bool>,
  pub lib_dirs: Strings<'a>,
  pub libs: Strings<'a>,

  // Platform specific
  pub android_target_api_level: Option<u8>,

  // Architecture specific
  pub arm_thumb_mode: Option<bool>
}

impl<'a> Settings<'a> {
  fn debug() -> Self {
    Settings {
      warning_level:      Some(3),
      warning_as_error:   Some(false),
      optimize:           Some(Optimize::None),
      strict_aliasing:    Some(false),
      omit_frame_pointer: Some(false),
      link_incremental:   Some(true),
      ..Default::default()
    }
  }

  fn release() -> Self {
    Settings {
      warning_level:      Some(3),
      warning_as_error:   Some(true),
      optimize:           Some(Optimize::Full),
      strict_aliasing:    Some(true),
      omit_frame_pointer: Some(true),
      link_incremental:   Some(false),
      ..Default::default()
    }
  }

  pub fn defaults() -> Profiles<'a> {
    let mk = |x: Settings<'a>| vec!(Profile::new(x));
    let mut m = Profiles::new();
    m.insert("Debug",   mk(Self::debug()));
    m.insert("Release", mk(Self::release()));
    m
  }

  pub fn merge_mut<'b>(&'b mut self, o: &'a Self) where 'a: 'b {
    merge_vecs_mut(&mut self.include_dirs, &o.include_dirs);

    merge_opt_mut(&mut self.warning_level,    &o.warning_level);
    merge_opt_mut(&mut self.warning_as_error, &o.warning_as_error);

    merge_opt_mut(&mut self.optimize,           &o.optimize);
    merge_opt_mut(&mut self.strict_aliasing,    &o.strict_aliasing);
    merge_opt_mut(&mut self.omit_frame_pointer, &o.omit_frame_pointer);

    merge_vecs_mut(&mut self.defines, &o.defines);
    merge_vecs_mut(&mut self.undefs,  &o.undefs);

    merge_opt_mut(&mut self.enable_exceptions, &o.enable_exceptions);

    merge_opt_mut(&mut self.enable_rtti,  &o.enable_rtti);
    merge_opt_mut(&mut self.c_standard,   &o.c_standard);
    merge_opt_mut(&mut self.cxx_standard, &o.cxx_standard);

    merge_opt_mut (&mut self.link_incremental, &o.link_incremental);
    merge_vecs_mut(&mut self.lib_dirs,         &o.lib_dirs);
    merge_vecs_mut(&mut self.libs,             &o.libs);
  }

  pub fn merge(&'a self, o: &'a Self) -> Self {
    Settings {
      include_dirs:     merge_vecs(&self.include_dirs, &o.include_dirs),

      warning_level:    self.warning_level.or(o.warning_level),
      warning_as_error: self.warning_as_error.or(o.warning_as_error),

      optimize:           self.optimize.or(o.optimize),
      strict_aliasing:    self.strict_aliasing.or(o.strict_aliasing),
      omit_frame_pointer: self.omit_frame_pointer.or(o.omit_frame_pointer),

      defines: merge_vecs(&self.defines, &o.defines),
      undefs:  merge_vecs(&self.undefs, &o.defines),

      enable_exceptions: self.enable_exceptions.or(o.enable_exceptions),

      enable_rtti:  self.enable_rtti.or(o.enable_rtti),
      c_standard:   self.c_standard.or(o.c_standard),
      cxx_standard: self.cxx_standard.or(o.cxx_standard),

      link_incremental: self.link_incremental.or(o.link_incremental),
      lib_dirs:         merge_vecs(&self.lib_dirs, &o.lib_dirs),
      libs:             merge_vecs(&self.libs, &o.libs),

      android_target_api_level: self.android_target_api_level.or(o.android_target_api_level),

      arm_thumb_mode: self.arm_thumb_mode.or(o.arm_thumb_mode)
    }
  }

  /*
  pub fn copy<'b, 'o>(&'b self) -> Settings<'o> where 'a: 'b, 'b: 'o {
    Settings {
      include_dirs: borrow_vec(&self.include_dirs),

      warning_level: self.warning_level,
      warning_as_error: self.warning_as_error,

      optimize: self.optimize,
      strict_aliasing: self.strict_aliasing,
      omit_frame_pointer: self.omit_frame_pointer,

      defines:      borrow_vec(&self.defines),
      undefs:       borrow_vec(&self.undefs),

      enable_exceptions: self.enable_exceptions,

      enable_rtti: self.enable_rtti,
      c_standard: self.c_standard,
      cxx_standard: self.cxx_standard,

      link_incremental: self.link_incremental,
      lib_dirs:     borrow_vec(&self.lib_dirs),
      libs:         borrow_vec(&self.libs),

      android_target_api_level: self.android_target_api_level,

      arm_thumb_mode: self.arm_thumb_mode
    }
  }
  */
}

/*
fn borrow_vec<'a, 'b, 'o>(a: &'b Strings<'a>) -> Strings<'o> where 'a: 'b, 'b: 'o {
  match a {
    None => None,
    Some(ac) => Some(Cow::Borrowed(&*ac))
  }
}
*/

fn merge_opt_mut<T: Copy>(a: &mut Option<T>, b: &Option<T>) {
  if let Some(bv) = b {
    *a = Some(*bv);
  }
}

fn merge_vecs_mut<'a, 'b>(a: &'b mut Strings<'a>, b: &'a Strings<'a>) where 'a: 'b {
  if a.is_empty() {
    if !b.is_empty() {
      *a = Cow::Borrowed(&*b);
    }
  }
  else {
    if !b.is_empty() {
      a.to_mut().extend(b.iter());
    }
  }
}

fn merge_vecs<'a>(a: &'a Strings, b: &'a Strings) -> Strings<'a> {
  if a.is_empty() {
    Cow::Borrowed(&*b)
  }
  else if b.is_empty() {
    Cow::Borrowed(&*a)
  }
  else {
    let mut v = a.to_vec();
    v.extend(&b[..]);
    v.into()
  }
}

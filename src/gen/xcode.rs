//! Project generator for XCode.
//!
//! XCode uses the NeXTSTEP property list format. The entire project is stored
//! in a single file named "project.pbxproj", short for Project Builder XCode
//! Project. This file lives in a folder named after the project with the
//! "xcodeproj" extension.
//!
//! This property list format provides the following data types:
//! - Number:     42
//! - String:     "contents"
//! - Array:      ( element, ... )
//! - Dictionary: { key = value; ... }
//!
//! Comments are also supported with the form /* contents */. They are
//! completely optional, and XCode will successfully load the project if they
//! are missing. However, comments are still generated for consistency; if the
//! generated project file is put in version control their presence limits
//! changes when the file is edited from XCode.
//!
//! Notes:
//! - Binary data is supported by the format but unused by XCode classes.
//! - Strings can omit the "" if they don't contain spaces or delimiters.
//!
//! The project.pbxproj file contains a single root element holding a dictionary
//! of every object describing the project along with some general information.
//! Every object is identified by a unique 96-bit hexadecimal string, and has an
//! "isa" property determining its type. Entries in this dictionary are grouped
//! by type, with comments as delimiters between different types. A comment is
//! also added after an object identifier to describe the object.
//!
//! ```
//! /* Begin <SECTION-NAME> section */
//! <OBJECT-ID> /* <OBJECT-NAME> */ = <OBJECT-PROPERTIES-DICTIONARY>,
//! ...
//! /* End <SECTION-NAME> section */
//! ```
//!
//! Note that the ordering of objects in this dictionary is not important for
//! XCode to load the project successfully. However, when XCode updates the
//! project file, objects are ordered by their identifier within their respective
//! sections.
//!
//! XCode supports the following object types as the value of the "isa" property:
//! - PBXProject                    The root object describing the project.
//! - PBXTarget
//!   - PBXAggregateTarget          A target aggregating several others.
//!   - PBXLegacyTarget             A target produced using an external build tool.
//!   - PBXNativeTarget             A target producing a native application or library.
//! - PBXTargetDependency           A PBXNativeTarget to PBXContainerItemProxy dependency.
//! - PBXContainerItemProxy         A reference to another object from the same workspace.
//! - PBXBuildFile                  A file reference used in a PBXBuildPhase.
//! - PBXFileElement
//!   - PBXFileReference            An external file referenced by the project.
//!   - PBXGroup                    Container for PBXFileReference and PBXGroup objects.
//!   - PBXVariantGroup             Gathers localized files for a PBXFileRefence object.
//! - PBXBuildPhase                 Describes a step in the build process.
//!   - PBXAppleScriptBuildPhase
//!   - PBXCopyFilesBuildPhase
//!   - PBXFrameworksBuildPhase
//!   - PBXHeadersBuildPhase
//!   - PBXResourcesBuildPhase
//!   - PBXShellScriptBuildPhase
//!   - PBXSourcesBuildPhase
//! - XCBuildConfiguration          Compiler, linker and target settings.
//! - XCConfigurationList           A list of XCBuildConfiguration objects.
//!
//! Note that only the concrete leaf types are used directly.
//! TODO: PBXBuildRule, PBXReferenceProxy
//!
//! References:
//! - https://en.wikipedia.org/wiki/Property_list
//! - http://monoobjc.net/xcode-project-file-format.html

use serde::Serialize;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs::{File, create_dir_all, remove_file};
use std::io::{BufWriter, Write as IOWrite};
use std::path::{Path, PathBuf};
use std::str::from_utf8;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::ctx::{Context, Generator, PlatformType, RunResult, StrError, Target, TargetType};

const PLATFORMS: &[PlatformType] = &[
  PlatformType::MacOS,
  PlatformType::IOS,
  PlatformType::TVOS,
  PlatformType::WatchOS
];

pub struct XCode;

impl Generator for XCode {
  fn supports_platform(&self, p: PlatformType) -> bool {
    assert!(p != PlatformType::Any);
    PLATFORMS.contains(&p)
  }

  // TODO check if any target matches before calling
  fn run(&self, ctx: &Context) -> RunResult {
    let team_output; // Declared here so it outlives the borrows in `team`.
    let team = match &ctx.env.jank_xcode_team {
      None => None,
      Some(name) => {
        team_output = std::process::Command::new("sh")
          .args(&["-c", format!(concat!("certtool y | ",
                                        "grep \"Org \\+: {}\" -B 1 | ",
                                        "head -n 1 | ",
                                        "awk '{{print $3}}'"),
                                name).as_str()])
          .output()?;

        if !team_output.status.success() {
          return Err(Box::new(StrError(["Failed to get the provisioning profile for '",
                                        name, "': ", from_utf8(&team_output.stderr)?].join(""))));
        }

        let team = from_utf8(&team_output.stdout)?.trim_end();
        if team.is_empty() {
          return Err(Box::new(StrError("Failed to get the provisioning profile".to_string())));
        }

        Some(team)
      }
    };

    let mut path = ctx.build_dir.join(&ctx.project.name);
    path.set_extension("xcodeproj");
    create_dir_all(&path)?;
    path.push("project.pbxproj");
    write_pbx(ctx, &path, team)?;
    Ok(())
  }
}

type IO = std::io::Result<()>;

static NEXT_ID_PREFIX: AtomicU32 = AtomicU32::new(0);

fn random_id() -> String {
  // TODO deterministic IDs? try and keep the same IDs between generator runs
  use rand::RngCore;
  let mut bytes: [u8; 12] = unsafe { std::mem::MaybeUninit::uninit().assume_init() };
  rand::thread_rng().fill_bytes(&mut bytes[4..]);

  // Use a counter as the first ID bytes to try and prevent Xcode from reordering objects.
  let prefix = NEXT_ID_PREFIX.fetch_add(1, Ordering::Relaxed);
  bytes[0] =  (prefix >> 24)         as u8;
  bytes[1] = ((prefix >> 16) & 0xFF) as u8;
  bytes[2] = ((prefix >> 8)  & 0xFF) as u8;
  bytes[3] =  (prefix        & 0xFF) as u8;

  let mut id = String::with_capacity(24);
  for b in &bytes {
    id.push(hex_char(b >> 4));
    id.push(hex_char(b & 0xF));
  }
  id
}

fn hex_char(b: u8) -> char {
  match b < 10 {
    true  => (b'0' + b)        as char,
    false => (b'A' + (b - 10)) as char
  }
}

fn quote(s: &str) -> Cow<'_, str> {
  match s.is_empty() || s.contains(' ') || s.contains('-') {
    true  => Cow::Owned(["\"", s, "\""].join("")),
    false => Cow::Borrowed(s)
  }
}

enum Phase {
  None,
  Source,
  Resource
}

/// Type used to resolve how many targets a file is a member of. This is used
/// when grouping files by target to generate the "Shared" group. Doing so is
/// required because Xcode only allows a PBXFileReference to be part of a single
/// PBXGroup. Additional file properties are also gathered here.
struct FileStats {
  id:          String,
  phase:       Phase,
  pbx_type:    &'static str,
  num_targets: u32
}

struct TargetData<'a> {
  target:       &'a Target<'a>,
  target_id:    String,
  target_name:  &'a str,
  product_id:   String,
  product_name: Cow<'a, str>,
  cfg_list:     CfgList,
  build_phases: String
}

struct Group<'a> {
  id:       String,
  name:     Option<&'a str>,
  path:     Option<&'a str>,
  children: String,
  groups:   Vec<Group<'a>>
}

impl<'a> Group<'a> {
  fn new(name: Option<&'a str>, path: Option<&'a str>) -> Self {
    Group {
      path,
      name,
      id:       String::new(),
      children: String::new(),
      groups:   Vec::new()
    }
  }

  fn is_empty(&self) -> bool {
    self.children.is_empty() && self.groups.is_empty()
  }

  fn get_name(&self) -> &'_ str {
    self.name.or(self.path).unwrap()
  }

  fn push(&mut self, id: &str, name: &str) {
    write!(&mut self.children, "\t\t\t\t{} /* {} */,\n", id, name).unwrap();
  }

  fn push_path(&mut self, id: &str, path: &'a Path) {
    let mut parts = path.iter();
    let mut curr  = parts.next().unwrap();
    let mut next  = parts.next();
    let mut group = self;
    loop {
      let name = curr.to_str().unwrap();
      match next {
        None => {
          group.push(id, name);
          break;
        },
        Some(x) => {
          curr  = x;
          next  = parts.next();
          group = match group.groups.iter().position(|x| x.path == Some(name)) {
            Some(i) => &mut group.groups[i],
            None    => {
              group.push_group(Group::new(None, Some(name)));
              group.groups.last_mut().unwrap()
            }
          };
        }
      }
    }
  }

  fn push_group(&mut self, child: Group<'a>) {
    self.groups.push(child);
  }

  fn write<W>(&mut self, f: &mut W) -> IO where W: IOWrite {
    for g in &mut self.groups {
      g.write(f)?;
    }

    self.id = random_id();

    match self.path.or(self.name) {
      None        => write!(f, "\t\t{} = {{\n",          self.id)?,
      Some(ident) => write!(f, "\t\t{} /* {} */ = {{\n", self.id, ident)?
    }

    f.write_all(concat!("\t\t\tisa = PBXGroup;\n",
                        "\t\t\tchildren = (\n").as_bytes())?;

    for g in &self.groups {
      write!(f, "\t\t\t\t{} /* {} */,\n", g.id, g.get_name())?;
    }

    f.write_all(self.children.as_bytes())?;
    f.write_all("\t\t\t);\n".as_bytes())?;

    if let Some(x) = self.path {
      write!(f, "\t\t\tpath = {};\n", quote(x))?;
    }

    if let Some(x) = &self.name {
      write!(f, "\t\t\tname = {};\n", quote(x))?;
    }

    f.write_all(concat!("\t\t\tsourceTree = \"<group>\";\n",
                        "\t\t};\n").as_bytes())?;

    Ok(())
  }
}

struct CfgList {
  id:   String,
  cfgs: String
}

impl CfgList {
  fn new() -> Self {
    CfgList {
      id:   random_id(),
      cfgs: String::new()
    }
  }

  fn push(&mut self, id: &str, name: &str) {
    write!(&mut self.cfgs, "\t\t\t\t{} /* {} */,\n", id, name).unwrap();
  }

  fn write<W>(&self, f: &mut W, kind: &str, name: &str) -> IO where W: IOWrite {
    write!(f, concat!("\t\t{id} /* Build configuration list for {kind} \"{name}\" */ = {{\n",
                      "\t\t\tisa = XCConfigurationList;\n",
                      "\t\t\tbuildConfigurations = (\n",
                      "{cfgs}",
                      "\t\t\t);\n",
                      "\t\t\tdefaultConfigurationIsVisible = 0;\n",
                      "\t\t\tdefaultConfigurationName = Release;\n",
                      "\t\t}};\n"),
           id   = self.id,
           kind = kind,
           name = name,
           cfgs = self.cfgs)?;
    Ok(())
  }
}

fn build_file(phase: &mut String, files: &mut String, file_name: &str,
              ref_id: &str, phase_name: &str)
{
  let id = random_id();
  write!(phase, "\t\t\t\t{} /* {} in {} */,\n", id, file_name, phase_name).unwrap();
  write!(files, concat!("\t\t{id} /* {name} in {phase} */ = {{",
                        "isa = PBXBuildFile; ",
                        "fileRef = {refid} /* {name} */; }};\n"),
         id    = id,
         name  = file_name,
         refid = ref_id,
         phase = phase_name).unwrap();
}

fn build_cfg<F>(cfg: &mut String, id: &str, name: &str, f: F) where F: FnOnce(&mut String) {
  write!(cfg, concat!("\t\t{} /* {} */ = {{\n",
                      "\t\t\tisa = XCBuildConfiguration;\n",
                      "\t\t\tbuildSettings = {{\n"),
         id, name).unwrap();

  f(cfg);

  write!(cfg, concat!("\t\t\t}};\n",
                      "\t\t\tname = {};\n",
                      "\t\t}};\n"),
         name).unwrap();
}

fn get_target_ext(t: TargetType) -> &'static str {
  match t {
    TargetType::Auto |
    TargetType::None |
    TargetType::Custom        => unreachable!(),
    TargetType::Console       => "",
    TargetType::Application   => ".app",
    TargetType::StaticLibrary => ".a",
    TargetType::SharedLibrary => ".dylib"
  }
}

fn get_file_type(ext: &'_ str) -> (Phase, &'static str) {
  match ext {
    "h"            => (Phase::None,     "sourcecode.c.h"),
    "hpp"          => (Phase::None,     "sourcecode.cpp.h"),
    "c"            => (Phase::Source,   "sourcecode.c"),
    "cc" | "cpp"   => (Phase::Source,   "sourcecode.cpp.cpp"),
    "m"            => (Phase::Source,   "sourcecode.c.objc"),
    "mm"           => (Phase::Source,   "sourcecode.cpp.objcpp"),
    "plist"        => (Phase::Resource, "text.plist.xml"),
    "bmp"          => (Phase::None,     "image.bmp"),
    "jpg" | "jpeg" => (Phase::None,     "image.jpeg"),
    "xml"          => (Phase::None,     "text.xml"),
    &_             => (Phase::None,     "text")
  }
}

fn write_info_plist(path: &Path) -> IO {
  let mut f = File::create(path)?;

  f.write_all(concat!(r#"<?xml version="1.0" encoding="UTF-8"?>"#, "\n",
                      r#"<!DOCTYPE plist PUBLIC "-//APPLE//DTD PLIST 1.0//EN" "#,
                      r#""http://www.apple.com/DTDs/PropertyList-1.0.dtd">"#, "\n",
                      r#"<plist version="1.0">"#, "\n",
                      "<dict>\n",
                      "  <key>CFBundleDevelopmentRegion</key>\n",
                      "  <string>${DEVELOPMENT_LANGUAGE}</string>\n",
                      "  <key>CFBundleExecutable</key>\n",
                      "  <string>${EXECUTABLE_NAME}</string>\n",
                      "  <key>CFBundleIdentifier</key>\n",
                      "  <string>${PRODUCT_BUNDLE_IDENTIFIER}</string>\n",
                      "  <key>CFBundleInfoDictionaryVersion</key>\n",
                      "  <string>6.0</string>\n",
                      "  <key>CFBundleName</key>\n",
                      "  <string>${PRODUCT_NAME}</string>\n",
                      "  <key>CFBundlePackageType</key>\n",
                      "  <string>${PRODUCT_BUNDLE_PACKAGE_TYPE}</string>\n",
                      "  <key>CFBundleShortVersionString</key>\n",
                      "  <string>1.0</string>\n",
                      "  <key>CFBundleVersion</key>\n",
                      "  <string>1</string>\n",
                      "</dict>\n",
                      "</plist>\n").as_bytes())?;

  f.flush()?;
  Ok(())
}

#[derive(Serialize)]
struct AssetInfo {
  version: u32,
  author:  &'static str
}

impl AssetInfo {
  fn new() -> Self {
    AssetInfo {
      version: 1,
      author:  "janky"
    }
  }
}

#[derive(Serialize)]
struct Asset {
  size:     &'static str,
  idiom:    &'static str,
  filename: &'static str,
  role:     &'static str
}

#[derive(Serialize)]
struct AssetLayer {
  filename: &'static str
}

fn str_is_empty(s: &str) -> bool {
  s.is_empty()
}

#[derive(Serialize)]
struct AssetImage<'a> {
  #[serde(skip_serializing_if = "str_is_empty")]
  size: &'a str,

  idiom:    &'a str,
  filename: String,
  scale:    &'static str,

  #[serde(skip)]
  path: &'a Path
}

#[derive(Serialize)]
struct AssetContent<'a> {
  #[serde(skip_serializing_if = "Vec::is_empty")]
  assets: Vec<Asset>,

  #[serde(skip_serializing_if = "Vec::is_empty")]
  layers: Vec<AssetLayer>,

  #[serde(skip_serializing_if = "Vec::is_empty")]
  images: Vec<AssetImage<'a>>,

  info: AssetInfo,

  #[serde(skip)]
  name: &'a str,

  #[serde(skip)]
  layer: u8,

  #[serde(skip)]
  children: Vec<AssetContent<'a>>
}

impl Default for AssetContent<'_> {
  fn default() -> Self {
    AssetContent {
      assets:   Vec::new(),
      layers:   Vec::new(),
      images:   Vec::new(),
      children: Vec::new(),
      info:     AssetInfo::new(),
      name:     "",
      layer:    0
    }
  }
}

impl<'a> AssetContent<'a> {
  fn child(&mut self, name: &'a str) -> &mut Self {
    match self.children.iter().position(|x| x.name == name) {
      Some(i) => &mut self.children[i],
      None    => {
        self.children.push(AssetContent { name, ..AssetContent::default() });
        self.children.last_mut().unwrap()
      }
    }
  }

  fn brand(&mut self, size: &'static str, role: &'static str, filename: &'static str) -> &mut Self {
    let mut brand = self.child("App Icon & Top Shelf Image.brandassets");
    if !brand.assets.iter().any(|x| x.filename == filename) {
      brand.assets.push(Asset { size, filename, role, idiom: "tv" });
    }
    brand.child(filename)
  }

  fn stack(&mut self, index: u8) -> &mut Self {
    let filename = match index {
      1 => "1.imagestacklayer",
      2 => "2.imagestacklayer",
      3 => "3.imagestacklayer",
      4 => "4.imagestacklayer",
      5 => "5.imagestacklayer",
      _ => unreachable!() // TODO better handling
    };

    if !self.layers.iter().any(|x| x.filename == filename) {
      self.layers.push(AssetLayer { filename });
    }

    self.child(filename).child("Content.imageset")
  }

  fn image(&mut self, idiom: &'a str, p: &ParsedAsset<'a>) {
    self.images.push(AssetImage {
      idiom,
      size:     p.size,
      path:     p.path,
      filename: p.path.file_name().unwrap().to_str().unwrap().to_string(),
      scale:    match p.scale {
        1 => "1x",
        2 => "2x",
        3 => "3x",
        _ => unreachable!() // TODO better handling
      }
    });
  }
}

fn fold_asset<'a, 'b>(asset: &'b mut AssetContent<'a>, p: &ParsedAsset<'a>) where 'a: 'b {
  // TODO reuse "App Icon", handle by platform
  match p.name {
    "icon" => {
      asset.child("AppIcon.appiconset").image("mac", p);
    },
    "AppIcon" => {
      asset.child("AppIcon.appiconset").image(p.idiom, p);
    },
    "App Icon" => {
      asset.brand("400x240", "primary-app-icon", "App Icon.imagestack")
        .stack(p.layer)
        .image("tv", p);
    }
    "App Icon - App Store" => {
      asset.brand("1280x768", "primary-app-icon", "App Icon - App Store.imagestack")
        .stack(p.layer)
        .image("tv", p);
    },
    "Top Shelf Image" => {
      asset.brand("1920x720", "top-shelf-image", "Top Shelf Image.imageset")
        .image("tv", p);
    },
    "Top Shelf Image Wide" => {
      asset.brand("2320x720", "top-shelf-image-wide", "Top Shelf Image Wide.imageset")
        .image("tv", p);
    },
    "Launch Image" => {
      // ???
    },
    &_ => {}
  }
}

#[derive(Debug)]
struct ParsedAsset<'a> {
  path:  &'a Path,
  name:  &'a str,
  size:  &'a str,
  idiom: &'a str,
  layer: u8,
  scale: u8
}

/// Parses information about an image asset from its filename. Note that very little
/// validation is performed here, instead delegating it to Xcode itself.
///
/// Supports all icons up to version 13.0 for macOS, iOS iphone/ipad, tvOS and watchOS.
///
/// macOS images:
/// - icon_[size].png
/// - icon_[size]@[scale].png
///
/// [size] is one of '16x16', '32x32', '128x128', '256x256' or '512x512'.
/// [scale] has to be '2x' or left out of the filename for '1x' images.
///
/// iOS images:
/// - [name]_[size]@[scale].png
/// - [name]_[idiom]_[size]@[scale].png
///
/// [name] is one of 'AppIcon' or 'LaunchImage'.
/// [idiom] is one of 'iphone', 'ipad' or unspecified for 'ios-marketing'.
/// [size] varies depending on the idiom:
/// - for iphone icons: one of '20x20', '29x29', '40x40', '60x60'.
/// - for ipad icons  : one of '20x20', '29x29', '40x40', '76x76', '83.5x83.5'.
/// [scale] is one of '2x' or '3x' for iphone icons, ipad also needs '1x'.
///
/// tvOS images:
/// - [name]@[scale].png
/// - [name] [layer]@[scale].png
///
/// [scale] is one of '1x' or '2x'.
///
/// [name] is one of:
/// - 'App Icon'
/// - 'App Icon - App Store'
/// - 'Top Shelf Image'
/// - 'Top Shelf Image Wide'
/// - 'Launch Image'
///
/// Note that image sizes are not part of the filename as they are inferred from the name.
///
/// watchOS images:
/// TODO
///
/// TODO ios launch images (orientation, idiom, extent, scale, minimum-system-version, subtype)
fn parse_asset<'a>(path: &'a Path, s: &'a str) -> Option<ParsedAsset<'a>> {
  let x = s.as_bytes();
  let e = x.len();
  if e < 10 || x[e - 4] != b'.' { // A 1@1x.png
    return None
  }

  {
    let ext = &x[e - 3 ..];
    if ext != b"jpg" && ext != b"png" {
      return None;
    }
  }

  // Parse the image scale (@1x, @2x, @3x).
  let scale;
  let offset = if x[e - 5] == b'x' && x[e - 6].is_ascii_digit() && x[e - 7] == b'@' {
    scale = x[e - 6] - b'0';
    e - 7
  }
  else {
    scale = 1;
    e - 4
  };

  // Parse the name, size, idiom and layer.
  let name;
  let size;
  let idiom;
  let layer = if x[offset - 1].is_ascii_digit() && x[offset - 2] == b' ' {
    idiom = "";
    size  = "";
    name  = from_utf8(&x[0 .. offset - 2]).unwrap();
    x[offset - 1] - b'0'
  }
  else {
    match x[0 .. offset].iter().position(|&c| c == b'_') {
      None => {
        idiom = "";
        size  = "";
        name  = from_utf8(&x[0 .. offset]).unwrap();
      },
      Some(pos) => {
        let size_pos = match x[pos + 1 .. offset].iter().position(|&c| c == b'_') {
          None => {
            idiom = "ios-marketing";
            pos
          },
          Some(sliced_pos) => {
            let idiom_pos = pos + sliced_pos + 1;
            idiom = from_utf8(&x[pos + 1 .. idiom_pos]).unwrap();
            idiom_pos
          }
        };
        size = from_utf8(&x[size_pos + 1 .. offset]).unwrap();
        name = from_utf8(&x[0 .. pos]).unwrap();
      }
    }
    0
  };

  Some(ParsedAsset { path, name, size, idiom, layer, scale })
}

fn write_contents_json(root: &Path, path: &Path, content: &AssetContent) -> IO {
  create_dir_all(&path)?;

  let mut f = BufWriter::new(File::create(path.join("Contents.json"))?);
  serde_json::to_writer_pretty(&mut f, content)?;
  f.flush()?;

  let src = pathdiff::diff_paths(&root, &path).unwrap();

  for image in &content.images {
    let target = path.join(image.path.file_name().unwrap());
    if target.symlink_metadata().is_ok() {
      remove_file(&target)?;
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(src.join(image.path), &target)?;

    // #[cfg(windows)]
    // std::os::windows::fs::symlink_file(src.join(image.path), &target)?;
  }

  for child in &content.children {
    write_contents_json(root, &path.join(child.name), &child)?;
  }

  Ok(())
}

const GROUP_REF: &str = "\"<group>\"";

fn write_file_ref(s: &mut String, id: &str, name: &str, path: Option<&Path>,
                  pbx_type: &str, source: &str)
{
  write!(s, concat!("\t\t{id} /* {name} */ = {{",
                    "isa = PBXFileReference; ",
                    "lastKnownFileType = {file}; "),
         id   = id,
         name = name,
         file = pbx_type).unwrap();

  if let Some(p) = path {
    write!(s, "name = {}; path = {}; ", quote(name), quote(p.to_str().unwrap())).unwrap();
  }
  else {
    write!(s, "path = {}; ", quote(name)).unwrap();
  }

  write!(s, "sourceTree = {}; }};\n", source).unwrap();
}

fn write_build_phase(s: &mut String, id: &str, phase: &str) {
  write!(s, concat!("\t\t{id} /* {phase} */ = {{\n",
                    "\t\t\tisa = PBX{phase}BuildPhase;\n",
                    "\t\t\tbuildActionMask = 2147483647;\n",
                    "\t\t\tfiles = (\n"),
         id    = id,
         phase = phase).unwrap();

}

fn pretty_name(prettify: bool, name: &str, platform: PlatformType) -> Cow<'_, str> {
  match prettify {
    true  => Cow::Owned([name, " (", platform.to_str(), ")"].join("")),
    false => Cow::from(name)
  }
}

fn sdk_info(p: PlatformType) -> (&'static str, &'static str) {
  match p {
    PlatformType::MacOS   => ("SDKROOT", ""),
    PlatformType::IOS     => ("DEVELOPER_DIR",
                              "Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS13.0.sdk/"),
    PlatformType::TVOS    => ("DEVELOPER_DIR",
                              "Platforms/AppleTVOS.platform/Developer/SDKs/AppleTVOS13.0.sdk/"),
    PlatformType::WatchOS => ("DEVELOPER_DIR",
                              "Platforms/WatchOS.platform/Developer/SDKs/WatchOS13.0.sdk/"),
    _                     => unreachable!()
  }
}

fn build_project_group<'a>(ctx: &Context, refs: &mut String) -> Group<'a> {
  let mut g = Group::new(Some("Project"), None);
  for f in ctx.metafiles {
    let id   = random_id();
    let name = f.name();
    write_file_ref(refs, &id, name, None, "text", GROUP_REF);
    g.push(&id, name);
  }
  g
}

fn write_pbx(ctx: &Context, path: &Path, team: Option<&str>) -> IO {
  // Open the file for writing right away to bail out early on failure.
  let mut f = BufWriter::new(File::create(path)?);

  // Prepare to collect all the required data to generate the PBX objects.
  let     project_id       = random_id();
  let mut project_cfgs     = CfgList::new();
  let mut cfgs             = String::new();
  let mut files            = String::new();
  let mut refs             = String::new();
  let mut sources          = String::new();
  let mut frameworks       = String::new();
  let mut resources        = String::new();
  let mut main_group       = Group::new(None, None);
  let mut shared_group     = Group::new(Some("Shared"), None);
  let mut product_group    = Group::new(Some("Products"), None);
  let mut frameworks_group = Group::new(Some("Frameworks"), None);
  let mut targets          = Vec::with_capacity(ctx.project.targets.len());

  for _ in 0..targets.capacity() {
    targets.push([None, None, None, None]);
  }

  // Collect information about files from every target.
  // At the same time, generate the shared group and file references.
  let file_stats = {
    let group = match ctx.project.info.xcode.group_by_target {
      true  => &mut shared_group,
      false => &mut main_group
    };

    ctx.sources.iter().flatten()
      .filter(|info| info.meta.is_file())
      .fold(HashMap::<&PathBuf, FileStats>::new(), |mut m, info| {
        m.entry(&info.path)
          .and_modify(|e| {
            if e.num_targets == 1 {
              group.push_path(&e.id, &info.path);
            }

            e.num_targets += 1;
          })
          .or_insert_with(|| {
            let id = random_id();
            let (phase, pbx_type) = get_file_type(info.extension());
            write_file_ref(&mut refs, &id, info.name(), None, pbx_type, GROUP_REF);
            FileStats { id, phase, pbx_type, num_targets: 1 }
          });
        m
      })
  };

  // let mut profiles = Vec::new();

  // Project build configurations.
  for prof in &ctx.profiles {
    // if let Some(p) = ctx.profiles.get(prof) {
    //   profiles.push(&p[0].settings);
    // }

    // profiles.push(&ctx.project.settings);

    // if let Some(p) = ctx.project.profiles.get(prof) {
    //   profiles.extend(p.iter().filter(|x| true).map(|x| &x.settings));
    // }

    let id = random_id();
    build_cfg(&mut cfgs, &id, prof, |s| {
      s.push_str("\t\t\t\tALWAYS_SEARCH_USER_PATHS = NO;\n"); // Deprecated, must be set to NO.

      // TODO dont hardcode
      let release   = *prof == "Release";
      let debug_fmt = match release {
        true  => "\"dwarf-with-dsym\"",
        false => "dwarf"
      };
      write!(s, concat!("\t\t\t\tCLANG_ANALYZER_NONNULL = YES;\n",
                        "\t\t\t\tCLANG_ANALYZER_NUMBER_OBJECT_CONVERSION = YES_AGGRESSIVE;\n",
                        "\t\t\t\tCLANG_CXX_LANGUAGE_STANDARD = \"gnu++17\";\n",
                        "\t\t\t\tCLANG_CXX_LIBRARY = \"libc++\";\n",
                        "\t\t\t\tCLANG_ENABLE_MODULES = YES;\n",
                        "\t\t\t\tCLANG_ENABLE_OBJC_ARC = YES;\n",
                        "\t\t\t\tCLANG_ENABLE_OBJC_WEAK = YES;\n",
                        "\t\t\t\tCOPY_PHASE_STRIP = NO;\n",
                        "\t\t\t\tDEBUG_INFORMATION_FORMAT = {};\n"),
             debug_fmt).unwrap();

      // TODO AVX2

      if release {
        s.push_str("\t\t\t\tENABLE_NS_ASSERTIONS = NO;\n");
      }

      s.push_str("\t\t\t\tENABLE_STRICT_OBJC_MSGSEND = YES;\n");

      if !release {
        s.push_str("\t\t\t\tENABLE_TESTABILITY = YES;\n");
      }

      write!(s, concat!("\t\t\t\tGCC_C_LANGUAGE_STANDARD = gnu11;\n",
                        "\t\t\t\tGCC_DYNAMIC_NO_PIC = NO;\n",
                        "\t\t\t\tGCC_ENABLE_CPP_EXCEPTIONS = NO;\n",
                        "\t\t\t\tGCC_ENABLE_CPP_RTTI = NO;\n" ,
                        "\t\t\t\tGCC_NO_COMMON_BLOCKS = YES;\n")).unwrap();

      let opt = match release {
        true  => "3",
        false => "0"
      };
      write!(s, "\t\t\t\tGCC_OPTIMIZATION_LEVEL = {};\n", opt).unwrap();

      let defines = match release {
        true  => &[] as &[&str],
        false => &["DEBUG=1"]
      };
      if !defines.is_empty() {
        s.push_str("\t\t\t\tGCC_PREPROCESSOR_DEFINITIONS = (\n");

        for d in defines {
          write!(s, "\t\t\t\t\t\"{}\",\n", d).unwrap();
        }

        s.push_str(concat!("\t\t\t\t\t\"$(inherited)\",\n",
                           "\t\t\t\t);\n"));
      }

      if !release {
        s.push_str("\t\t\t\tONLY_ACTIVE_ARCH = YES;\n");
      }
      else {
        s.push_str("\t\t\t\tLLVM_LTO = YES;\n");
      }

      write!(s, concat!("\t\t\t\tWARNING_CFLAGS = (\n",
                        "\t\t\t\t\t\"-Wall\",\n",
                        "\t\t\t\t\t\"-Wextra\",\n",
                        "\t\t\t\t\t\"-Wpedantic\",\n",
                        "\t\t\t\t);\n")).unwrap();
    });
    project_cfgs.push(&id, prof);
    // profiles.clear();
  }

  // Gather data for all the supported target/platform pairs.
  for (target_index, (target_name, target)) in ctx.project.targets.iter().enumerate() {
    let platforms = PLATFORMS.iter().cloned().enumerate()
      .filter(|&(_, p)| {
        // TODO also filter away unsupported architectures here?
        ctx.project.filter.matches_platform(p) && target.filter.matches_platform(p)
      }).collect::<Vec<(usize, PlatformType)>>();

    let has_multiple_platforms = platforms.len() > 1;
    let target_files = &ctx.sources[target_index];
    let data = &mut targets[target_index];

    let mut target_group = Group::new(Some(target_name), None);
    let group = match ctx.project.info.xcode.group_by_target {
      true  => &mut target_group,
      false => &mut main_group
    };

    for file_info in target_files {
      if file_info.meta.is_dir() {continue}
      let file = &file_stats[&file_info.path];
      if file.num_targets == 1 {
        group.push_path(&file.id, &file_info.path);
      }
    }

    for (platform_index, platform) in platforms {
      let mut cfg_list       = CfgList::new();
      let mut build_phases   = String::new();

      let settings_info_plist;
      let settings_app_icon;

      // Initialize the target's build phases.
      {
        let sources_id    = random_id();
        let frameworks_id = random_id();
        let resources_id  = random_id();

        write_build_phase(&mut sources,    &sources_id,    "Sources");
        write_build_phase(&mut frameworks, &frameworks_id, "Frameworks");
        write_build_phase(&mut resources,  &resources_id,  "Resources");

        write!(&mut build_phases, concat!("\t\t\t\t{} /* Sources */,\n",
                                          "\t\t\t\t{} /* Frameworks */,\n",
                                          "\t\t\t\t{} /* Resources */,\n"),
               sources_id, frameworks_id, resources_id).unwrap();
      }

      // Link frameworks
      let (sdk_source, sdk_prefix) = sdk_info(platform);
      let link_frameworks = match platform { // TODO dont hardcode
        PlatformType::WatchOS => &[] as &[&str],
        PlatformType::MacOS   => &["AppKit", "CoreVideo", "Metal", "OpenGL"],
        _                     => &["UIKit", "Metal", "OpenGLES", "QuartzCore"]
      };

      for lf in link_frameworks {
        let ref_id = random_id();
        let name = [lf, ".framework"].join("");
        let path = PathBuf::from([sdk_prefix, "System/Library/Frameworks/", &name].join(""));
        frameworks_group.push(&ref_id, &name);
        build_file(&mut frameworks, &mut files, &name, &ref_id, "Frameworks");
        write_file_ref(&mut refs, &ref_id, &name, Some(&path), "wrapper.framework", sdk_source);
      }

      // Generate application assets.
      if target.target_type == TargetType::Application {
        let gen_dir = PathBuf::from([target_name, "_", platform.to_str()].join(""));

        // TODO don't generate info.plist if it exists in assets
        let plist = gen_dir.join("Info.plist");
        create_dir_all(&gen_dir)?;
        write_info_plist(&ctx.build_dir.join(&plist))?;

        let plist_name   = pretty_name(has_multiple_platforms, "Info.plist", platform);
        let plist_ref    = ctx.build_rel.join(plist);
        let plist_ref_id = random_id();
        group.push(&plist_ref_id, &plist_name);
        write_file_ref(&mut refs, &plist_ref_id, &plist_name, Some(&plist_ref),
                       "text.plist.xml", GROUP_REF);

        settings_info_plist = format!("\t\t\t\tINFOPLIST_FILE = {};\n",
                                      quote(plist_ref.to_str().unwrap()));

        if let Some(dir) = target.assets {
          let platform_pattern = match platform {
            PlatformType::MacOS   => "/macos/",
            PlatformType::IOS     => "/ios/",
            PlatformType::TVOS    => "/tvos/",
            PlatformType::WatchOS => "/watchos/",
            _                     => unreachable!()
          };
          let assets_name    = pretty_name(has_multiple_platforms, "Assets.xcassets", platform);
          let assets_pattern = [dir, platform_pattern].join("");
          let assets = ctx.assets[target_index].iter()
            .filter(|info| info.meta.is_file() && info.to_str().starts_with(&assets_pattern))
            .map   (|info| parse_asset(&info.path, &info.to_str()[assets_pattern.len() ..]))
            .flatten()
            .fold(AssetContent {
              name: &assets_name,
              ..AssetContent::default()
            }, |mut assets, parsed| {
              fold_asset(&mut assets, &parsed); // TODO generic platform
              assets
            });

          let assets_path = gen_dir.join("Assets.xcassets");
          write_contents_json(&ctx.input_dir, &ctx.build_dir.join(&assets_path), &assets)?;

          let assets_ref    = ctx.build_rel.join(assets_path);
          let assets_ref_id = random_id();
          group.push(&assets_ref_id, assets.name);
          build_file(&mut resources, &mut files, &assets_name, &assets_ref_id, "Resources");
          write_file_ref(&mut refs, &assets_ref_id, &assets_name, Some(&assets_ref),
                         "folder.assetcatalog", GROUP_REF);

          settings_app_icon = format!("\t\t\t\tASSETCATALOG_COMPILER_APPICON_NAME = {};\n",
                                      match platform {
                                        PlatformType::MacOS   |
                                        PlatformType::IOS     |
                                        PlatformType::WatchOS => "AppIcon",
                                        PlatformType::TVOS    => "\"App Icon & Top Shelf Image\"",
                                        _                     => unreachable!()
                                      });
        }
        else {
          settings_app_icon = String::new();
        }
      }
      else {
        settings_info_plist = String::new();
        settings_app_icon   = String::new();
      }

      // Generate the build configurations for this target.
      for prof in &ctx.profiles {
        let id = random_id();
        build_cfg(&mut cfgs, &id, prof, |s| {
          s.push_str(&settings_app_icon);

          if target.target_type == TargetType::Application {
            s.push_str("\t\t\t\tCODE_SIGN_STYLE = Automatic;\n");
          }

          if let Some(id) = team {
            write!(s, "\t\t\t\tDEVELOPMENT_TEAM = {};\n", id).unwrap();
          }

          s.push_str(&settings_info_plist);

          // TODO libraries
          // DYLIB_COMPATIBILITY_VERSION = 1;
          // DYLIB_CURRENT_VERSION = 1;
          // EXECUTABLE_PREFIX = lib;
          // SKIP_INSTALL = YES;

          // TODO ???
          // OTHER_LDFLAGS = "-ObjC";
          // VALIDATE_PRODUCT = YES;

          // TODO frameworks
          // CURRENT_PROJECT_VERSION = 1;
          // DEFINE_MODULES = YES;
          // DYLIB_INSTALL_NAME_BASE = "@rpath";
          // LD_RUNPATH_SEARCH_PATHS = (
          //   "$(inherited)",
          //   "@executable_path/Frameworks",
          //   "@loader_path/Frameworks",
          // );
          // PRODUCT_BUNDLE_IDENTIFIER
          // PRODUCT_NAME = "$(TARGET_NAME:c99extidentifier)";
          // VERSIONING_SYSTEM = "apple-generic";
          // VERSION_INFO_PREFIX = "";

          let sdk;
          let family;
          let sdk_version;

          match platform { // TODO target version
            PlatformType::MacOS => {
              // TODO COMBINE_HIDPI_IMAGES = YES;
              sdk    = "macosx";
              family = "";
              sdk_version = "\t\t\t\tMACOSX_DEPLOYMENT_TARGET = 10.10;\n";
            },
            PlatformType::IOS => {
              sdk    = "iphoneos";
              family = "\"1,2\""; // TODO iphone vs ipad
              sdk_version = "\t\t\t\tIPHONEOS_DEPLOYMENT_TARGET = 10.0;\n";
            },
            PlatformType::TVOS => {
              sdk    = "appletvos";
              family = "3";
              sdk_version = "\t\t\t\tTVOS_DEPLOYMENT_TARGET = 10.0;\n";
            },
            PlatformType::WatchOS => {
              sdk    = "watchos";
              family = "4";
              sdk_version = "\t\t\t\tWATCHOS_DEPLOYMENT_TARGET = 6.0;\n";
            },
            _ => unreachable!(),
          }

          if platform == PlatformType::IOS {
            s.push_str(sdk_version);
          }

          s.push_str(concat!("\t\t\t\tLD_RUNPATH_SEARCH_PATHS = (\n",
                             "\t\t\t\t\t\"$(inherited)\",\n",
                             "\t\t\t\t\t\"@executable_path/Frameworks\",\n"));

          if platform != PlatformType::MacOS {
            s.push_str("\t\t\t\t\t\"@loader_path/Frameworks\",\n");
          }

          s.push_str("\t\t\t\t);\n");

          if platform == PlatformType::MacOS {
            s.push_str(sdk_version);
          }

          write!(s, concat!("\t\t\t\tPRODUCT_BUNDLE_IDENTIFIER = com.lambdacoder.Jank;\n",
                            "\t\t\t\tPRODUCT_NAME = {};\n"),
                 quote(target_name)).unwrap();

          write!(s, "\t\t\t\tSDKROOT = {};\n", sdk).unwrap();

          if !family.is_empty() {
            write!(s, "\t\t\t\tTARGETED_DEVICE_FAMILY = {};\n", family).unwrap();
          }

          if platform == PlatformType::TVOS || platform == PlatformType::WatchOS {
            s.push_str(sdk_version);
          }

          // TODO compiler
          // CLANG_ANALYZER_NONNULL = YES;
          // CLANG_ANALYZER_NUMBER_OBJECT_CONVERSION = YES_AGGRESSIVE;
          // CLANG_CXX_LANGUAGE_STANDARD = "gnu++14";
          // CLANG_CXX_LIBRARY = "libc++";
          // CLANG_ENABLE_MODULES = YES;
          // CLANG_ENABLE_OBJC_ARC = YES;
          // CLANG_ENABLE_OBJC_WEAK = YES;
          // CLANG_WARN_BLOCK_CAPTURE_AUTORELEASING = YES;
          // CLANG_WARN_BOOL_CONVERSION = YES;
          // CLANG_WARN_COMMA = YES;
          // CLANG_WARN_CONSTANT_CONVERSION = YES;
          // CLANG_WARN_DEPRECATED_OBJC_IMPLEMENTATIONS = YES;
          // ....

          // COPY_PHASE_STRIP = NO;
          // DEBUG_INFORMATION_FORMAT = dwarf;
          // ENABLE_STRICT_OBJC_MSGSEND = YES;
          // ENABLE_TESTABILITY = YES;

          // GCC_C_LANGUAGE_STANDARD = gnu11;
          // GCC_DYNAMIC_NO_PIC = NO;
          // GCC_NO_COMMON_BLOCKS = YES;
          // GCC_OPTIMIZATION_LEVEL = 0;
          // GCC_PREPROCESSOR_DEFINITIONS = ("DEBUG=1", "$(inherited)", );

          // MTL_ENABLE_DEBUG_INFO = INCLUDE_SOURCE;
          // MTL_FAST_MATH = YES;

          // ONLY_ACTIVE_ARCH = YES;

          // GCC_ENABLE_CPP_EXCEPTIONS = NO;
          // GCC_ENABLE_CPP_RTTI = NO;
        });
        cfg_list.push(&id, prof);
        // profiles.clear();
      }

      // Generate the build files for this target.
      for file_info in target_files {
        if file_info.meta.is_dir() {continue} // TODO
        let name = file_info.name();
        let file = &file_stats[&file_info.path];

        match file.phase {
          Phase::None     => {},
          Phase::Source   => build_file(&mut sources,   &mut files, name, &file.id, "Sources"),
          Phase::Resource => build_file(&mut resources, &mut files, name, &file.id, "Resources")
        }
      }

      // Finalize the target's build phase objects.
      const BUILD_PHASE_END: &str = concat!("\t\t\t);\n",
                                            "\t\t\trunOnlyForDeploymentPostprocessing = 0;\n",
                                            "\t\t};\n");
      sources.push_str(BUILD_PHASE_END);
      frameworks.push_str(BUILD_PHASE_END);
      resources.push_str(BUILD_PHASE_END);

      // Generate the target's product.
      let product_id   = random_id();
      let product_name = pretty_name(has_multiple_platforms, target_name, platform);
      let target_ext   = get_target_ext(target.target_type);
      write!(&mut refs, concat!("\t\t{product_id} /* {comment_name} */ = {{",
                                "isa = PBXFileReference; ",
                                "explicitFileType = {target_type}; ",
                                "includeInIndex = 0; ",
                                "name = {product_name}; ",
                                "path = {target_name}{target_ext}; ", // TODO quote over ext
                                "sourceTree = BUILT_PRODUCTS_DIR; }};\n"),
             product_id   = product_id,
             product_name = quote(&product_name),
             comment_name = &product_name,
             target_name  = target_name,
             target_ext   = target_ext,
             target_type  = match target.target_type {
               TargetType::Auto          |
               TargetType::None          |
               TargetType::Custom        => unreachable!(),
               TargetType::Application   => "wrapper.application",
               TargetType::Console       => "compiled.mach-o.executable",
               TargetType::SharedLibrary => "compiled.mach-o.dylib",
               TargetType::StaticLibrary => "archive.ar"
               // "text.plist.info"
               // "text.man"
               // "text"
             }).unwrap();

      write!(&mut product_group.children, "\t\t\t\t{} /* {} */,\n",
             product_id, product_name).unwrap();

      // Finalize this target.
      data[platform_index] = Some(TargetData {
        target_id: random_id(),
        target,
        target_name,
        product_id,
        product_name,
        cfg_list,
        build_phases
      });
    }

    if ctx.project.info.xcode.group_by_target && !target_group.is_empty() {
      main_group.push_group(target_group);
    }
  }

  if ctx.project.info.xcode.group_by_target && !shared_group.is_empty() {
    main_group.push_group(shared_group);
  }

  main_group.push_group(build_project_group(ctx, &mut refs));

  if !frameworks_group.is_empty() {
    main_group.push_group(frameworks_group);
  }

  main_group.push_group(product_group);

  // Finally, generate the project file.
  write!(f, concat!("// !$*UTF8*$!\n",
                    "{{\n",
                    "\tarchiveVersion = 1;\n",
                    "\tclasses = {{\n",
                    "\t}};\n",
                    "\tobjectVersion = 50;\n",
                    "\tobjects = {{\n",
                    "\n",
                    "/* Begin PBXBuildFile section */\n",
                    "{files}",
                    "/* End PBXBuildFile section */\n",
                    "\n",
                    "/* Begin PBXFileReference section */\n",
                    "{refs}",
                    "/* End PBXFileReference section */\n",
                    "\n",
                    "/* Begin PBXFrameworksBuildPhase section */\n",
                    "{frameworks}",
                    "/* End PBXFrameworksBuildPhase section */\n",
                    "\n",
                    "/* Begin PBXGroup section */\n"),
         files = files,
         refs  = refs,
         frameworks = frameworks)?;

  main_group.write(&mut f)?;

  f.write_all(concat!("/* End PBXGroup section */\n",
                  "\n",
                  "/* Begin PBXNativeTarget section */\n").as_bytes())?;

  for data in targets.iter().flatten().flatten() {
    write!(f, concat!("\t\t{target_id} /* {comment_name} */ = {{\n",
                      "\t\t\tisa = PBXNativeTarget;\n",
                      "\t\t\tbuildConfigurationList = {cfg_list_id} /* ",
                      "Build configuration list for PBXNativeTarget \"{comment_name}\" */;\n",
                      "\t\t\tbuildPhases = (\n",
                      "{build_phases}",
                      "\t\t\t);\n",
                      "\t\t\tbuildRules = (\n",
                      "\t\t\t);\n",
                      "\t\t\tdependencies = (\n",
                      "\t\t\t);\n",
                      "\t\t\tname = {product_name};\n",
                      "\t\t\tproductName = {product_name};\n",
                      "\t\t\tproductReference = {product_id} /* {comment_name} */;\n",
                      "\t\t\tproductType = \"com.apple.product-type.{product_type}\";\n",
                      "\t\t}};\n"),
           target_id    = data.target_id,
           product_id   = data.product_id,
           product_name = quote(&data.product_name),
           comment_name = &data.product_name,
           cfg_list_id  = data.cfg_list.id,
           build_phases = data.build_phases,
           product_type = match data.target.target_type {
             TargetType::Auto |
             TargetType::None |
             TargetType::Custom        => unreachable!(),
             TargetType::Console       => "tool",
             TargetType::Application   => "application",
             TargetType::StaticLibrary => "library.static",
             TargetType::SharedLibrary => "library.dynamic",
           })?;
  }

  write!(f, concat!("/* End PBXNativeTarget section */\n",
                    "\n",
                    "/* Begin PBXProject section */\n",
                    "\t\t{project_id} /* Project object */ = {{\n",
                    "\t\t\tisa = PBXProject;\n",
                    "\t\t\tattributes = {{\n",
                    "\t\t\t\tBuildIndependentTargetsInParallel = YES;\n",
                    "\t\t\t\tLastUpgradeCheck = 1100;\n",
                    "\t\t\t\tORGANIZATIONNAME = {organization};\n",
                    "\t\t\t\tTargetAttributes = {{\n"),
         project_id   = project_id,
         organization = quote("com.lambdacoder"))?;

  for data in targets.iter().flatten().flatten() {
    write!(f, concat!("\t\t\t\t\t{target_id} = {{\n",
                      "\t\t\t\t\t\tCreatedOnToolsVersion = 11.0;\n",
                      "\t\t\t\t\t}};\n"),
           target_id = data.target_id)?;
  }

  write!(f, concat!("\t\t\t\t}};\n",
                    "\t\t\t}};\n",
                    "\t\t\tbuildConfigurationList = {cfg_list_id} /* ",
                    "Build configuration list for PBXProject \"{project_name}\" */;\n",
                    "\t\t\tcompatibilityVersion = \"Xcode 9.3\";\n",
                    "\t\t\tdevelopmentRegion = en;\n",
                    "\t\t\thasScannedForEncodings = 0;\n",
                    "\t\t\tknownRegions = (\n"),
         cfg_list_id  = project_cfgs.id,
         project_name = ctx.project.name)?;

  for region in ["en", "Base"].iter() {
    write!(f, "\t\t\t\t{},\n", region)?;
  }

  write!(f, concat!("\t\t\t);\n",
                    "\t\t\tmainGroup = {main_group_id};\n",
                    "\t\t\tproductRefGroup = {product_group_id} /* Products */;\n",
                    "\t\t\tprojectDirPath = {project_dir_path};\n",
                    "\t\t\tprojectRoot = \"\";\n",
                    "\t\t\ttargets = (\n"),
         main_group_id    = main_group.id,
         product_group_id = main_group.groups.last().unwrap().id,
         project_dir_path = quote(ctx.input_rel.to_str().unwrap()))?;

  for data in targets.iter().flatten().flatten() {
    write!(f, "\t\t\t\t{} /* {} */,\n", data.target_id, &data.product_name)?;
  }

  // let variants = ""; // TODO
  write!(f, concat!("\t\t\t);\n",
                    "\t\t}};\n",
                    "/* End PBXProject section */\n",
                    "\n",
                    "/* Begin PBXResourcesBuildPhase section */\n",
                    "{resources}",
                    "/* End PBXResourcesBuildPhase section */\n",
                    "\n",
                    "/* Begin PBXSourcesBuildPhase section */\n",
                    "{sources}",
                    "/* End PBXSourcesBuildPhase section */\n",
                    "\n",
                    // "/* Begin PBXVariantGroup section */\n",
                    // "{variants}",
                    // "/* End PBXVariantSection section */\n",
                    // "\n",
                    "/* Begin XCBuildConfiguration section */\n",
                    "{cfgs}",
                    "/* End XCBuildConfiguration section */\n",
                    "\n",
                    "/* Begin XCConfigurationList section */\n"),
         resources = resources,
         sources   = sources,
         // variants  = variants,
         cfgs      = cfgs)?;

  project_cfgs.write(&mut f, "PBXProject", &ctx.project.name)?;

  for data in targets.iter().flatten().flatten() {
    data.cfg_list.write(&mut f, "PBXNativeTarget", &data.product_name)?;
  }

  write!(f, concat!("/* End XCConfigurationList section */\n",
                    "\t}};\n",
                    "\trootObject = {project_id} /* Project object */;\n",
                    "}}\n"),
         project_id = project_id)?;

  f.flush()?;
  Ok(())
}

// TODO deployment targets

// TODO build settings

// TODO target dependencies
// TODO legacy targets
// TODO shell script build phases

// TODO framework build file settings
// - *.framework in Embed Frameworks; settings = {ATTRIBUTES = (CodeSignOnCopy, RemoveHeadersOnCopy, ); };

// TODO library header build files
// - *.h in CopyFiles
// - *.h in Headers; settings = {ATTRIBUTES = (Public, ); };

// TODO PBXHeadersBuildPhase
// ???? for all library header files?

// TODO support storyboards

// TODO PBXCopyFilesBuildPhase
// {} /* CopyFiles */ = {
//   isa
//   buildActionMask = 2147483647;
//   dstPath = "include/$(PRODUCT_NAME)";
//   dstSubfolderSpec = 16;
//   files = ();
//   runOnlyForDeploymentPostprocessing = 0;
// };
// {} = /* Embed Frameworks */ = {
//   isa = PBSCopyFilesBuildPhase;
//   buildActionMask = 2147483647;
//   dstPath = "";
//   dstSubfolderSpec = 10;
//   files = ();
//   name = "Embed Frameworks";
//   runOnlyForDeploymentPostprocessing = 0;
// };

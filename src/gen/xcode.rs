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

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs::{File, create_dir_all};
use std::io::Write as IOWrite;
use std::path::PathBuf;

use crate::ctx::{Context, Generator, PlatformType, RunResult, Target, TargetType};

const PLATFORMS: [PlatformType; 3] = [
  PlatformType::MacOS,
  PlatformType::IOS,
  PlatformType::TVOS
];

pub struct XCode;

impl Generator for XCode {
  fn supports_platform(&self, p: PlatformType) -> bool {
    assert!(p != PlatformType::Any);
    PLATFORMS.contains(&p)
  }

  // TODO check if any target matches before calling
  fn run(&self, ctx: &Context) -> RunResult {
    let mut proj_dir = ctx.build_dir.join(&ctx.project.name);
    proj_dir.set_extension("xcodeproj");
    create_dir_all(&proj_dir)?;
    write_pbx(ctx, &proj_dir)?;
    Ok(())
  }
}

type IO = std::io::Result<()>;

fn random_id() -> String {
  // TODO semi-random IDs, try and prevent xcode from reordering objects
  use rand::RngCore;
  let mut bytes: [u8; 12] = unsafe { std::mem::MaybeUninit::uninit().assume_init() };
  rand::thread_rng().fill_bytes(&mut bytes);

  let mut id = String::with_capacity(24);
  for b in &bytes {
    id.push(hex_char(b & 0xF));
    id.push(hex_char(b >> 4));
  }
  id
}

fn hex_char(b: u8) -> char {
  if b < 10 {
    (b'0' + b) as char
  }
  else {
    (b'A' + (b - 10)) as char
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
  target:        &'a Target<'a>,
  target_rename: Cow<'a, str>,
  target_id:     String,
  product_id:    String,
  cfg_list:      CfgList,
  build_phases:  String
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
      id:       random_id(),
      children: String::new(),
      groups:   Vec::new()
    }
  }

  fn get_name(&self) -> &'_ str {
    self.name.or(self.path).unwrap()
  }

  fn push(&mut self, id: &str, name: &str) {
    write!(&mut self.children, "        {} /* {} */,\n", id, name).unwrap();
  }

  fn push_group(&mut self, child: Group<'a>) {
    self.push(&child.id, child.get_name());
    self.groups.push(child);
  }

  fn write(&self, f: &mut File) -> IO {
    write!(f, concat!("    {} = {{\n",
                      "      isa = PBXGroup;\n",
                      "      children = (\n",
                      "{}",
                      "      );\n"),
           self.id, self.children)?;

    if let Some(x) = self.path {
      write!(f, "      path = \"{}\";\n", x)?;
    }

    if let Some(x) = &self.name {
      write!(f, "      name = \"{}\";\n", x)?;
    }

    write!(f, concat!("      sourceTree = \"<group>\";\n",
                      "    }};\n"))?;

    for g in &self.groups {
      g.write(f)?;
    }

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
    write!(&mut self.cfgs, "        {} /* {} */,\n", id, name).unwrap();
  }

  fn write(&self, f: &mut File, kind: &str, name: &str) -> IO {
    write!(f, concat!("    {} /* Build configuration list for {} \"{}\" */ = {{\n",
                      "      isa = XCConfigurationList;\n",
                      "      buildConfigurations = (\n",
                      "{}",
                      "      );\n",
                      "      defaultConfigurationIsVisible = 0;\n",
                      "      defaultConfigurationName = Release;\n",
                      "    }};\n"),
           self.id, kind, name, self.cfgs)?;
    Ok(())
  }
}

fn build_file(phase: &mut String, files: &mut String, file_name: &'_ str,
                  ref_id: &'_ str, phase_name: &'_ str)
{
  let id = random_id();
  write!(phase, "        {} /* {} in {} */,\n", id, file_name, phase_name).unwrap();
  write!(files, concat!("    {0} /* {1} in {3} */ = {{",
                        "isa = PBXBuildFile; ",
                        "fileRef = {2} /* {1} */; }};\n"),
         id, file_name, ref_id, phase_name).unwrap();
}

fn build_cfg<F>(cfg: &mut String, id: &str, name: &str, f: F) where F: FnOnce(&mut String) {
  write!(cfg, concat!("    {0} /* {1} */ = {{\n",
                      "      isa = XCBuildConfiguration;\n",
                      "      buildSettings = {{\n"),
         id, name).unwrap();

  f(cfg);

  write!(cfg, concat!("      }};\n",
                      "      name = {0};\n",
                      "    }};\n"),
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

fn write_pbx(ctx: &Context, proj_dir: &PathBuf) -> IO {
  // Open the file for writing right away to bail out early on failure.
  let mut f = File::create(proj_dir.join("project.pbxproj"))?;

  // Prepare to collect all the required data to generate the PBX objects.
  let     project_id    = random_id();
  let mut project_cfgs  = CfgList::new();
  let mut cfgs          = String::new();
  let mut files         = String::new();
  let mut refs          = String::new();
  let mut sources       = String::new();
  let mut resources     = String::new();
  let mut main_group    = Group::new(None, None);
  let mut shared_group  = Group::new(Some("Shared"), None);
  let mut product_group = Group::new(Some("Products"), None);
  let mut targets       = Vec::with_capacity(ctx.project.targets.len());

  for _ in 0..targets.capacity() {
    targets.push([None, None, None]);
  }

  // Collect information about files from every target.
  // At the same time, generate the shared group and file references.
  let file_stats = {
    let group = if ctx.project.info.xcode.group_by_target {
      &mut shared_group
    }
    else {
      &mut main_group
    };

    ctx.files.iter().flatten()
      .fold(HashMap::<&PathBuf, FileStats>::new(), |mut m, info| {
        m.entry(&info.0)
          .and_modify(|e| {
            if e.num_targets == 1 {
              group.push(&e.id, info.name());
            }

            e.num_targets += 1;
          })
          .or_insert_with(|| {
            let id = random_id();
            let (phase, pbx_type) = get_file_type(info.extension());
            write!(&mut refs, concat!("    {0} /* {1} */ = {{",
                                      "isa = PBXFileReference; ",
                                      "lastKnownFileType = {3}; ",
                                      "name = {1}; ",
                                      "path = {2:?}; ",
                                      "sourceTree = \"<group>\"; }};\n"),
                   id, info.name(), info.path(), pbx_type).unwrap();
            FileStats { id, phase, pbx_type, num_targets: 1 }
          });
        m
      })
  };

  // let mut profiles = Vec::new();

  // Project build configurations.
  let profile_names = ctx.profile_names();

  for prof in &profile_names {
    // if let Some(p) = ctx.profiles.get(prof) {
    //   profiles.push(&p[0].settings);
    // }

    // profiles.push(&ctx.project.settings);

    // if let Some(p) = ctx.project.profiles.get(prof) {
    //   profiles.extend(p.iter().filter(|x| true).map(|x| &x.settings));
    // }

    let id = random_id();
    build_cfg(&mut cfgs, &id, prof, |s| {
      write!(s, concat!("        ALWAYS_SEARCH_USER_PATHS = NO;\n",
                        "        PRODUCT_NAME = \"$(TARGET_NAME)\";\n")).unwrap();
    });
    project_cfgs.push(&id, prof);
    // profiles.clear();
  }

  // Gather data for all the supported target/platform pairs.
  for (target_index, (target_name, target)) in ctx.project.targets.iter().enumerate() {
    let platforms: Vec<(usize, PlatformType)> = PLATFORMS.iter().cloned().enumerate()
      .filter(|(_, p)| {
        // TODO also filter away unsupported platform architectures here?
        ctx.project.filter.matches_platform(p) && target.filter.matches_platform(p)
      })
      .collect();

    let has_multiple_platforms = platforms.len() > 1;
    let target_files = &ctx.files[target_index];
    let data = &mut targets[target_index];

    let mut target_group = Group::new(Some(target_name), None);
    {
      let group = if ctx.project.info.xcode.group_by_target {
        &mut target_group
      }
      else {
        &mut main_group
      };

      for file_info in target_files {
        let file = &file_stats[&file_info.0];
        if file.num_targets == 1 {
          group.push(&file.id, file_info.name()); // TODO map folders to sub-groups
        }
      }
    }

    for (platform_index, platform) in platforms {
      let mut cfg_list     = CfgList::new();
      let mut build_phases = String::new();

      // Generate the build configurations for this target.
      for prof in &profile_names {
        let id = random_id();
        build_cfg(&mut cfgs, &id, prof, |s| {
          // TODO PRODUCT_NAME ?
          // TODO DEVELOPMENT_TEAM

          if target.target_type == TargetType::Application {
            // TODO get app icon resource
            // TODO get plist for platform
            write!(s, concat!("        ASSETCATALOG_COMPILER_APPICON_NAME = \"\";\n",
                              "        CODE_SIGN_STYLE = Automatic;\n",
                              "        INFOPLIST_FILE = \"\";\n")).unwrap();
          }

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
          // INFOPLIST_FILE = "{}";
          // LD_RUNPATH_SEARCH_PATHS = (
          //   "$(inherited)",
          //   "@executable_path/Frameworks",
          //   "@loader_path/Frameworks",
          // );
          // PRODUCT_BUNDLE_IDENTIFIER
          // PRODUCT_NAME = "$(TARGET_NAME:c99extidentifier)";
          // VERSIONING_SYSTEM = "apple-generic";
          // VERSION_INFO_PREFIX = "";

          match platform {
            PlatformType::MacOS => {
              // TODO COMBINE_HIDPI_IMAGES = YES;
              write!(s, "        MACOSX_DEPLOYMENT_TARGET = 10.14;\n").unwrap();
              write!(s, "        SDKROOT = macosx;\n").unwrap();
            },
            PlatformType::IOS => {
              write!(s, "        IPHONEOS_DEPLOYMENT_TARGET = 13.0;\n").unwrap();
              write!(s, "        SDKROOT = iphoneos;\n").unwrap();
              write!(s, "        TARGETED_DEVICE_FAMILY = \"1,2\";\n").unwrap(); // TODO iphone vs ipad
            },
            PlatformType::TVOS => {
              write!(s, "        IPHONEOS_DEPLOYMENT_TARGET = 13.0;\n").unwrap();
              write!(s, "        SDKROOT = appletvos;\n").unwrap();
              write!(s, "        TARGETED_DEVICE_FAMILY = 3;\n").unwrap();
            },
            _ => unreachable!(),
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

      // Initialize the target's build phases.
      let sources_id   = random_id();
      let resources_id = random_id(); // TODO frameworks too?
      write!(&mut sources, concat!("    {} /* Sources */ = {{\n",
                                   "      isa = PBXSourcesBuildPhase;\n",
                                   "      buildActionMask = 2147483647;\n",
                                   "      files = (\n"),
             sources_id).unwrap();

      // TODO other phases
      write!(&mut build_phases, "        {} /* Sources */,\n", sources_id).unwrap();

      let target_rename = if has_multiple_platforms {
        Cow::Owned([target_name, " (", platform.to_str(), ")"].join(""))
      }
      else {
        Cow::from(*target_name)
      };

      // Generate the build files for this target.
      for file_info in target_files {
        let name = file_info.name();
        let file = &file_stats[&file_info.0];

        match file.phase {
          Phase::None     => {},
          Phase::Source   => build_file(&mut sources,   &mut files, name, &file.id, "Sources"),
          Phase::Resource => build_file(&mut resources, &mut files, name, &file.id, "Resources")
        }
      }

      // Finalize the target's build phase objects.
      // TODO other phases
      sources.push_str(concat!("      );\n",
                               "      runOnlyForDeploymentPostprocessing = 0;\n",
                               "    };\n"));

      // Generate the target's product.
      let target_id   = random_id();
      let product_id  = random_id();
      let product_ext = get_target_ext(target.target_type);
      write!(&mut refs, concat!("    {0} /* {1}{2} */ = {{",
                                "isa = PBXFileReference; ",
                                "explicitFileType = {3}; ",
                                "includeInIndex = 0; ",
                                "path = \"{1}{2}\"; ",
                                "sourceTree = BUILT_PRODUCTS_DIR; }};\n"),
             product_id, target_name, product_ext,
             match target.target_type {
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

      write!(&mut product_group.children, "        {} /* {}{} */,\n",
             product_id, target_name, product_ext).unwrap();

      // Finalize this target.
      data[platform_index] = Some(TargetData {
        target,
        target_rename,
        target_id,
        product_id,
        cfg_list,
        build_phases
      });
    }

    if ctx.project.info.xcode.group_by_target {
      main_group.push_group(target_group);
    }
  }

  if ctx.project.info.xcode.group_by_target {
    main_group.push_group(shared_group);
  }

  main_group.push_group(product_group);

  let frameworks = ""; // TODO

  // Finally, generate the project file.
  write!(f, concat!("// !$*UTF8*$!\n",
                    "{{\n",
                    "  archiveVersion = 1;\n",
                    "  classes = {{\n",
                    "  }};\n",
                    "  objectVersion = 50;\n",
                    "  objects = {{\n",
                    "\n",
                    "/* Begin PBXBuildFile section */\n",
                    "{}",
                    "/* End PBXBuildFile section */\n",
                    "\n",
                    "/* Begin PBXFileReference section */\n",
                    "{}",
                    "/* End PBXFileReference section */\n",
                    "\n",
                    "/* Begin PBXFrameworksBuildPhase section */\n",
                    "{}",
                    "/* End PBXFrameworksBuildPhase section */\n",
                    "\n",
                    "/* Begin PBXGroup section */\n"),
         files, refs, frameworks)?;

  main_group.write(&mut f)?;

  write!(f, concat!("/* End PBXGroup section */\n",
                    "\n",
                    "/* Begin PBXNativeTarget section */\n"))?;

  for data in targets.iter().flatten().flatten() {
    write!(f, concat!("    {0} /* {1} */ = {{\n",
                      "      isa = PBXNativeTarget;\n",
                      "      buildConfigurationList = {4} /* ",
                      "Build configuration list for PBXNativeTarget \"{1}\" */;\n",
                      "      buildPhases = (\n",
                      "{3}",
                      "      );\n",
                      "      buildRules = (\n",
                      "      );\n",
                      "      dependencies = (\n",
                      "      );\n",
                      "      name = \"{1}\";\n",
                      "      productName = \"{1}\";\n",
                      "      productReference = {5} /* {1}{2} */;\n",
                      "      productType = \"com.apple.product-type.{6}\";\n",
                      "    }};\n"),
           data.target_id, &data.target_rename, get_target_ext(data.target.target_type),
           data.build_phases, data.cfg_list.id, data.product_id,
           match data.target.target_type {
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
                    "    {} /* Project object */ = {{\n",
                    "      isa = PBXProject;\n",
                    "      attributes = {{\n",
                    "        BuildIndependentTargetsInParallel = YES;\n",
                    "        LastUpgradeCheck = 1100;\n",
                    "        ORGANIZATIONNAME = \"{}\";\n",
                    "        TargetAttributes = {{\n"),
         project_id, "com.lambdacoder")?;

  for data in targets.iter().flatten().flatten() {
    write!(f, concat!("          {} = {{\n",
                      "            CreatedOnToolsVersion = 11.0;\n",
                      "          }};\n"),
           data.target_id)?;
  }

  write!(f, concat!("        }};\n",
                    "      }};\n",
                    "      buildConfigurationList = {} /* ",
                    "Build configuration list for PBXProject \"{}\" */;\n",
                    "      compatibilityVersion = \"Xcode 9.3\";\n",
                    "      developmentRegion = en;\n",
                    "      hasScannedForEncodings = 0;\n",
                    "      knownRegions = (\n"),
         project_cfgs.id, ctx.project.name)?;

  for region in ["en", "Base"].iter() {
    write!(f, "       {},\n", region)?;
  }

  write!(f, concat!("      );\n",
                    "      mainGroup = {0};\n",
                    "      productRefGroup = {1} /* Products */;\n",
                    "      projectDirPath = {2:?};\n",
                    "      projectRoot = \"\";\n",
                    "      targets = (\n"),
         main_group.id, main_group.groups.last().unwrap().id,
         pathdiff::diff_paths(&ctx.input_dir, &ctx.build_dir).unwrap())?;

  for data in targets.iter().flatten().flatten() {
      write!(f, "        {} /* {} */,\n", data.target_id, &data.target_rename)?;
  }

  let variants = ""; // TODO
  write!(f, concat!("      );\n",
                    "    }};\n",
                    "/* End PBXProject section */\n",
                    "\n",
                    "/* Begin PBXResourcesBuildPhase section */\n",
                    "{}",
                    "/* End PBXResourcesBuildPhase section */\n",
                    "\n",
                    "/* Begin PBXSourcesBuildPhase section */\n",
                    "{}",
                    "/* End PBXSourcesBuildPhase section */\n",
                    "\n",
                    "/* Begin PBXVariantGroup section */\n",
                    "{}",
                    "/* End PBXVariantSection section */\n",
                    "\n",
                    "/* Begin XCBuildConfiguration section */\n",
                    "{}",
                    "/* End XCBuildConfiguration section */\n",
                    "\n",
                    "/* Begin XCConfigurationList section */\n"),
         resources, sources, variants, cfgs)?;

  project_cfgs.write(&mut f, "PBXProject", &ctx.project.name)?;

  for data in targets.iter().flatten().flatten() {
    data.cfg_list.write(&mut f, "PBXNativeTarget", &data.target_rename)?;
  }

  write!(f, concat!("/* End XCConfigurationList section */\n",
                    "  }};\n",
                    "  rootObject = {} /* Project object */;\n",
                    "}}\n"),
         project_id)?;

  f.flush()?;
  Ok(())
}

// TODO target app icons
// TODO deployment targets

// TODO build settings

// TODO target dependencies
// TODO legacy targets
// TODO shell script build phases

// TODO framework build file settings
// - *.framework in Frameworks
// - *.framework in Embed Frameworks; settings = {ATTRIBUTES = (CodeSignOnCopy, RemoveHeadersOnCopy, ); };

// TODO library header build files
// - *.h in CopyFiles
// - *.h in Headers; settings = {ATTRIBUTES = (Public, ); };

// TODO PBXFrameworksBuildPhase

// TODO PBXHeadersBuildPhase
// ???? for all library header files?

// TODO PBXResourcesBuildPhase
// - storyboards, xcassets

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

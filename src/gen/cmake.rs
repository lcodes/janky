use std::borrow::Cow;
use std::fs::{File, create_dir_all};
use std::io::{BufWriter, Write};

use crate::ctx::{Context, Generator, PlatformType, RunResult, Target, TargetType};

const PLATFORMS: [PlatformType; 3] = [
  PlatformType::Android,
  PlatformType::HTML5,
  PlatformType::Linux
];

pub struct CMake;

impl Generator for CMake {
  fn supports_platform(&self, p: PlatformType) -> bool {
    assert!(p != PlatformType::Any);
    PLATFORMS.contains(&p)
  }

  fn run(&self, ctx: &Context) -> RunResult {
    if !PLATFORMS.iter().any(|x| ctx.project.filter.matches_platform(*x)) {
      return Ok(());
    }

    let input_rel    = ctx.input_rel.join("..");
    let mut sources  = Vec::with_capacity(ctx.sources.len());
    let mut includes = Vec::with_capacity(ctx.sources.len());

    let targets = ctx.project.targets.iter().enumerate().map(|(index, (name, target))| {
      let builds = PLATFORMS.iter().map(move |&platform| {
        match target.filter.matches_platform(platform) {
          false => None,
          true  => {
            Some(Build {
              name, target, index,
              platform: platform,
              path:     [name, "_", platform.to_str()].join("")
            })
          }
        }
      }).flatten().collect::<Vec<Build>>();

      let mut s = String::new();
      let mut i = String::new();
      if !builds.is_empty() {
        for src in ctx.sources[index].iter().filter(|x| x.is_source_no_objc()) {
          s.push_str("  ");
          s.push_str(input_rel.join(&src.path).to_str().unwrap());
          s.push('\n');
        };

        let mut incs = Vec::new();
        for inc in ctx.sources[index].iter().filter(|x| x.is_header()) {
          incs.push(inc.path.parent().unwrap());
        }

        incs.dedup();

        for inc in incs {
          i.push_str("  ");
          i.push_str(input_rel.join(inc).to_str().unwrap());
          i.push('\n');
        }
      }

      sources.push(s);
      includes.push(i);
      builds
    }).flatten().collect::<Vec<Build>>();

    for build in targets {
      write_lists_txt(ctx, &build, &TargetInfo {
        sources:  &sources[build.index],
        includes: &includes[build.index]
      })?;
    }

    Ok(())
  }
}

type IO = std::io::Result<()>;

struct TargetInfo<'a> {
  sources:  &'a String,
  includes: &'a String
}

struct Build<'a> {
  index:    usize,
  path:     String,
  name:     &'a str,
  target:   &'a Target<'a>,
  platform: PlatformType
}

fn write_lists_txt(ctx: &Context, build: &Build, info: &TargetInfo) -> IO {
  let mut f = BufWriter::new(File::create({
    let mut path = ctx.build_dir.join(&build.path);
    create_dir_all(&path)?;
    path.push("CMakeLists.txt");
    path
  })?);

  let (target_type, target_subtype) = match build.target.target_type {
    TargetType::Application => {
      match build.platform {
        PlatformType::Android => ("library",    " SHARED"),
        _                     => ("executable", "")
      }
    },
    TargetType::StaticLibrary => ("library", " STATIC"),
    TargetType::SharedLibrary => ("library", " SHARED"),
    _ => unreachable!()
  };

  let sources: Cow<'_, str> = match build.platform { // TODO dont hardcode
    PlatformType::Android =>
      Cow::Owned([info.sources,
                  "  ${ANDROID_NDK}/sources/android/native_app_glue/android_native_app_glue.c\n"].join("")),
    _ => Cow::Borrowed(&info.sources)
  };

  let includes: Cow<'_, str> = match build.platform { // TODO dont hardcode
    PlatformType::Android =>
      Cow::Owned([info.includes,
                  concat!("  ${ANDROID_NDK}/sources/android/native_app_glue\n",
                          "  ${ANDROID_NDK}/sources/third_party/shaderc/include\n")].join("")),
    _ => Cow::Borrowed(&info.includes)
  };

  let libraries = match build.platform { // TODO dont hardcode
    PlatformType::Android =>
      concat!("  android\n",
              "  log\n",
              "  EGL\n",
              "  GLESv3\n",
              "  vulkan\n",
              "  ${ANDROID_NDK}/sources/third_party/shaderc/libs/c++_static/${ANDROID_ABI}/libshaderc.a\n"),
    PlatformType::HTML5 =>
      concat!("  openal\n",
              "  websocket.js\n"),
    PlatformType::Linux => "",
    _ => unreachable!()
  };

  let cmake_version = "3.10.2"; // TODO dont hardcode
  write!(f, concat!("cmake_minimum_required(VERSION {})\n",
                    "project({})\n\n"),
         cmake_version, build.name)?;

  if build.platform == PlatformType::HTML5 {
    f.write(concat!("if (NOT ${CMAKE_SYSTEM_NAME} MATCHES \"Emscripten\")\n",
                    "  message(FATAL_ERROR \"Failed to detect Emscripten: run with 'emcmake cmake .'\")\n",
                    "endif ()\n\n",
                    "set(CMAKE_EXECUTABLE_SUFFIX \".html\")\n",
                    "set(CMAKE_RUNTIME_OUTPUT_DIRECTORY \"${CMAKE_CURRENT_SOURCE_DIR}/dist\")\n\n"
    ).as_bytes())?;

    // TODO hardcoded
    let flags = concat!(" -s WASM=1",
                        " -s USE_WEBGL2=1",
                        " -s EXIT_RUNTIME=1",
                        " -s ASSERTIONS=2",
                        " -s DISABLE_DEPRECATED_FIND_EVENT_TARGET_BEHAVIOR=1",
                        " --emrun",
                        " --preload-file ../../demo");
    write!(f, "set(CMAKE_EXE_LINKER_FLAGS \"${{CMAKE_EXE_LINKER_FLAGS}}{}\")\n\n", flags)?;
  }

  // TODO hardcoded flags
  let flags         = "-Wall -Wextra -Wpedantic -fno-exceptions -fno-rtti";
  let release_flags = "-Werror";
  write!(f, concat!("set(CMAKE_CXX_FLAGS \"{flags}\")\n",
                    "set(CMAKE_CXX_FLAGS_MINSIZEREL \"{release_flags}\")\n",
                    "set(CMAKE_CXX_FLAGS_RELWITHDEBINFO \"{release_flags}\")\n",
                    "set(CMAKE_CXX_FLAGS_RELEASE \"{release_flags}\")\n\n",
                    "add_{target_type}({target_name}{target_subtype}\n",
                    "{sources}",
                    "  )\n\n",
                    "set_target_properties({target_name} PROPERTIES\n",
                    "  CXX_STANDARD 17\n",
                    "  CXX_STANDARD_REQUIRED YES\n",
                    "  CXX_EXTENSIONS NO\n",
                    "  )\n\n",
                    "target_include_directories({target_name} PRIVATE\n",
                    "{includes}",
                    "  )\n\n",
                    "target_link_libraries({target_name} PRIVATE\n",
                    "{libraries}",
                    "  )\n"),
         flags          = flags,
         release_flags  = release_flags,
         target_name    = build.name,
         target_type    = target_type,
         target_subtype = target_subtype,
         sources        = sources,
         includes       = includes,
         libraries      = libraries)?;

  f.flush()?;
  Ok(())
}

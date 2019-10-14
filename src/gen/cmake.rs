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

    let targets = ctx.project.targets.iter().enumerate().map(|(index, (name, target))| {
      PLATFORMS.iter().map(move |&platform| {
        match target.filter.matches_platform(platform) {
          false => None,
          true  => {
            Some(Build {
              name, target, index, platform,
              path: [name, "_", platform.to_str()].join("")
            })
          }
        }
      }).flatten()
    }).flatten();

    for build in targets {
      write_lists_txt(ctx, &build)?;
    }

    Ok(())
  }
}

type IO = std::io::Result<()>;

struct Build<'a> {
  index:    usize,
  path:     String,
  name:     &'a str,
  target:   &'a Target<'a>,
  platform: PlatformType
}

fn write_lists_txt(ctx: &Context, build: &Build) -> IO {
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

  let sources = match build.platform { // TODO dont hardcode
    PlatformType::Android => "  ${ANDROID_NDK}/sources/android/native_app_glue/android_native_app_glue.c\n",
    _ => ""
  };

  let includes = match build.platform { // TODO dont hardcode
    PlatformType::Android =>
      concat!("  ${ANDROID_NDK}/sources/android/native_app_glue\n",
              "  ${ANDROID_NDK}/sources/third_party/shaderc/include\n"),
    _ => ""
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
    f.write_all(concat!("if (NOT ${CMAKE_SYSTEM_NAME} MATCHES \"Emscripten\")\n",
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

  let rel    = ctx.input_rel.join("..");
  let prefix = rel.to_str().unwrap();
  let files  = &ctx.sources[build.index];

  let srcs = files.iter().filter(|x| {
    x.is_source_no_objc() && build.target.match_file(&x.path, build.platform)
  });

  let mut incs = files.iter().filter(|x| {
    x.is_header() && build.target.match_file(&x.path, build.platform)
  }).map(|x| x.path.parent().unwrap().to_str().unwrap())
    .collect::<Vec<&str>>();

  incs.dedup();

  // TODO hardcoded flags
  let flags         = "-Wall -Wextra -Wpedantic -fno-exceptions -fno-rtti";
  let release_flags = "-Werror";
  write!(f, concat!("set(CMAKE_CXX_FLAGS \"{flags}\")\n",
                    "set(CMAKE_CXX_FLAGS_MINSIZEREL \"{release_flags}\")\n",
                    "set(CMAKE_CXX_FLAGS_RELWITHDEBINFO \"{release_flags}\")\n",
                    "set(CMAKE_CXX_FLAGS_RELEASE \"{release_flags}\")\n\n",
                    "add_{target_type}({target_name}{target_subtype}\n"),
         flags          = flags,
         release_flags  = release_flags,
         target_name    = build.name,
         target_type    = target_type,
         target_subtype = target_subtype)?;

  for src in srcs {
    write!(f, "  {}/{}\n", prefix, src.to_str())?;
  }
  f.write_all(sources.as_bytes())?;

  write!(f, concat!("  )\n\n",
                    "set_target_properties({target_name} PROPERTIES\n",
                    "  CXX_STANDARD 17\n",
                    "  CXX_STANDARD_REQUIRED YES\n",
                    "  CXX_EXTENSIONS NO\n",
                    "  )\n\n",
                    "target_include_directories({target_name} PRIVATE\n"),
         target_name = build.name)?;

  for src in incs {
    write!(f, "  {}/{}\n", prefix, src)?;
  }
  f.write_all(includes.as_bytes())?;

  write!(f, concat!("  )\n\n",
                    "target_link_libraries({target_name} PRIVATE\n",
                    "{libraries}",
                    "  )\n"),
         target_name    = build.name,
         libraries      = libraries)?;

  f.flush()?;
  Ok(())
}

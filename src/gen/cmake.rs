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

    let input_rel = ctx.input_rel.join("..");
    let mut sources = Vec::with_capacity(ctx.sources.len());

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
      if !builds.is_empty() {
        for src in ctx.sources[index].iter().filter(|x| x.meta.is_file()) {
          s.push_str("  ");
          s.push_str(input_rel.join(&src.path).to_str().unwrap());
          s.push('\n');
        };
      }

      sources.push(s);
      builds
    }).flatten().collect::<Vec<Build>>();

    for build in targets {
      write_lists_txt(ctx, &build, &sources[build.index])?;
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

fn write_lists_txt(ctx: &Context, build: &Build, sources: &String) -> IO {
  let mut f = BufWriter::new(File::create({
    let mut path = ctx.build_dir.join(&build.path);
    create_dir_all(&path)?;
    path.push("CMakeLists.txt");
    path
  })?);

  let (target_type, target_subtype) = match build.target.target_type {
    TargetType::Application => {
      match build.platform {
        PlatformType::Android => ("library",     " SHARED"),
        _                     => ("application", "")
      }
    },
    TargetType::StaticLibrary => ("library", " STATIC"),
    TargetType::SharedLibrary => ("library", " SHARED"),
    _ => unreachable!()
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
              "  native_app_glue\n",
              "  vulkan\n",
              "  ${ANDROID_NDK}/sources/third_party/shaderc/libs/c++_static/${ANDROID_ABI}/libshaderc.a\n"),
    PlatformType::HTML5 => "",
    PlatformType::Linux => "",
    _ => unreachable!()
  };

  write!(f, concat!("cmake_minimum_required(VERSION {cmake_version})\n\n",
                    "set(CMAKE_CXX_FLAGS \"${{CMAKE_CXX_FLAGS}} -Wall -Werror -fno-exceptions -fno-rtti\")\n\n",
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
                    "  )\n\n"),
         cmake_version  = "3.10.2",
         target_name    = build.name,
         target_type    = target_type,
         target_subtype = target_subtype,
         sources        = sources,
         includes       = includes,
         libraries      = libraries)?;

  f.flush()?;
  Ok(())
}

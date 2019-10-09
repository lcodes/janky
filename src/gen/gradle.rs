use std::fs::File;
use std::io::Write;

use crate::ctx::{Context, Generator, PlatformType, RunResult};

pub struct Gradle;

impl Generator for Gradle {
  fn supports_platform(&self, p: PlatformType) -> bool {
    match p {
      PlatformType::Any     => unreachable!(),
      PlatformType::Android => true,
      PlatformType::IOS     => false,
      PlatformType::Linux   => false,
      PlatformType::MacOS   => false,
      PlatformType::TVOS    => false,
      PlatformType::WatchOS => false,
      PlatformType::Windows => false,
      PlatformType::HTML5   => false
    }
  }

  fn run(&self, ctx: &Context) -> RunResult {
    // <target>/build.gradle

    write_root_build(&ctx)?;
    write_properties(&ctx)?;
    write_settings(ctx)?;
    Ok(())
  }
}

type IO = std::io::Result<()>;

fn write_target_build(ctx: &Context) -> IO {
  Ok(())
}

fn write_root_build(ctx: &Context) -> IO {
  let mut f = File::create(ctx.build_dir.join("build.gradle"))?;
  f.write(concat!("buildscript {\n",
                  "  repositories {\n",
                  "    google()\n",
                  "    jcenter()\n",
                  "  }\n\n",
                  "  dependencies {\n",
                  "    classpath 'com.android.tools.build:gradle:3.5.0'\n",
                  "  }\n",
                  "}\n\n",
                  "allprojects {\n",
                  "  repositories {\n",
                  "    google()\n",
                  "    jcenter()\n",
                  "  }\n",
                  "}\n\n",
                  "task clean(type: Delete) {\n",
                  "  delete rootProject.buildDir\n",
                  "}\n").as_bytes())?;
  Ok(())
}

fn write_properties(ctx: &Context) -> IO {
  let mut f = File::create(ctx.build_dir.join("gradle.properties"))?;
  f.write(b"org.gradle.jvmargs=-Xmx8g\n")?;
  Ok(())
}

fn write_settings(ctx: &Context) -> IO {
  let mut f = File::create(ctx.build_dir.join("settings.gradle"))?;
  f.write(b"include ")?;
  // ':target1', ':target2', ...
  f.write(b"\n")?;
  Ok(())
}

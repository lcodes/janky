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
      PlatformType::Windows => false
    }
  }

  fn run(&self, _ctx: &Context) -> RunResult {
    Ok(())
  }
}

// build.gradle

/*

android {
  defaultConfig {}
  buildTypes {}

  externalNativeBuild {
    cmake {
      path "CMakeLists.txt",
      version "3.10.2"
    }
  }
}

*/

use crate::ctx::{Context, Generator, PlatformType, RunResult};

pub struct Make;

impl Generator for Make {
  fn supports_platform(&self, p: PlatformType) -> bool {
    match p {
      PlatformType::Any     => unreachable!(),
      PlatformType::Android => false,
      PlatformType::IOS     => false,
      PlatformType::Linux   => true,
      PlatformType::MacOS   => false,
      PlatformType::TVOS    => false,
      PlatformType::WatchOS => false,
      PlatformType::Windows => false,
      PlatformType::HTML5   => true
    }
  }

  fn run(&self, _ctx: &Context) -> RunResult {
    Ok(())
  }
}

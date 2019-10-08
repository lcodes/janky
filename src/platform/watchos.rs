use crate::{ctx, ctx::{Architecture, PlatformType}};

pub struct WatchOS;

impl ctx::Platform for WatchOS {
  fn get_platform_type(&self) -> PlatformType {
    PlatformType::WatchOS
  }

  fn supports_architecture(&self, a: Architecture) -> bool {
    match a {
      Architecture::Any   => unreachable!(),
      Architecture::ARM   => false,
      Architecture::ARM64 => true,
      Architecture::X86   => false,
      Architecture::X64   => false
    }
  }

  fn run(&self, _ctx: &ctx::Context) -> ctx::RunResult {
    Ok(())
  }
}

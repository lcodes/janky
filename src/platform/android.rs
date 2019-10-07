use crate::{ctx, ctx::{Architecture, PlatformType}};

pub struct Android;

impl ctx::Platform for Android {
  fn get_platform_type(&self) -> PlatformType {
    PlatformType::Android
  }

  fn supports_architecture(&self, a: Architecture) -> bool {
    match a {
      Architecture::Any   => unreachable!(),
      Architecture::ARM   => true,
      Architecture::ARM64 => true,
      Architecture::X86   => true,
      Architecture::X64   => false
    }
  }

  fn run(&self, _ctx: &ctx::Context) -> ctx::RunResult {
    Ok(())
  }
}

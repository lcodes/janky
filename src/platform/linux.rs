use crate::{ctx, ctx::{Architecture, PlatformType}};

pub struct Linux;

impl ctx::Platform for Linux {
  fn get_platform_type(&self) -> PlatformType {
    PlatformType::Linux
  }

  fn supports_architecture(&self, a: Architecture) -> bool {
    match a {
      Architecture::Any   => unreachable!(),
      Architecture::ARM   => false,
      Architecture::ARM64 => false,
      Architecture::X86   => true,
      Architecture::X64   => true
    }
  }

  fn run(&self, _ctx: &ctx::Context) -> ctx::RunResult {
    Ok(())
  }
}

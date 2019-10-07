use crate::{ctx, ctx::{Architecture, PlatformType}};

pub struct MacOS;

impl ctx::Platform for MacOS {
  fn get_platform_type(&self) -> PlatformType {
    PlatformType::MacOS
  }

  fn supports_architecture(&self, a: Architecture) -> bool {
    match a {
      Architecture::Any   => unreachable!(),
      Architecture::ARM   => false,
      Architecture::ARM64 => false,
      Architecture::X86   => false,
      Architecture::X64   => true
    }
  }

  fn run(&self, _ctx: &ctx::Context) -> ctx::RunResult {
    Ok(())
  }
}

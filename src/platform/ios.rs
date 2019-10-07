use crate::{ctx, ctx::{Architecture, PlatformType}};

pub struct IOS;

impl ctx::Platform for IOS {
  fn get_platform_type(&self) -> PlatformType {
    PlatformType::IOS
  }

  fn supports_architecture(&self, a: Architecture) -> bool {
    match a {
      Architecture::Any   => unreachable!(),
      Architecture::ARM   => true,
      Architecture::ARM64 => true,
      Architecture::X86   => false,
      Architecture::X64   => false
    }
  }

  fn run(&self, _ctx: &ctx::Context) -> ctx::RunResult {
    Ok(())
  }
}

use crate::{ctx, ctx::{Architecture, PlatformType}};

pub struct Windows;

impl ctx::Platform for Windows {
  fn get_platform_type(&self) -> PlatformType {
    PlatformType::Windows
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

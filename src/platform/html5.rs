use crate::{ctx, ctx::{Architecture, PlatformType}};

pub struct HTML5;

impl ctx::Platform for HTML5 {
  fn get_platform_type(&self) -> PlatformType {
    PlatformType::HTML5
  }

  fn supports_architecture(&self, a: Architecture) -> bool {
    match a {
      Architecture::Any   => unreachable!(),
      Architecture::ARM   => false,
      Architecture::ARM64 => false,
      Architecture::X86   => false,
      Architecture::X64   => false
    }
  }

  fn run(&self, _ctx: &ctx::Context) -> ctx::RunResult {
    Ok(())
  }
}

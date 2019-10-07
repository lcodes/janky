use clap::{App};

use crate::ctx::{Command, Context, RunResult};

pub struct Gen;

impl Command for Gen {
  fn init<'a, 'b>(&self, cmd: App<'a, 'b>) -> App<'a, 'b> {
    cmd.about("Generates the project's build files")
  }

  fn run(&self, ctx: &Context) -> RunResult {
    ctx.generators.get("xcode").unwrap().run(ctx)
  }
}

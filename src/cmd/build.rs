use clap::{App};

use crate::ctx::{Command, Context, RunResult};

pub struct Build;

impl Command for Build {
  fn init<'a, 'b>(&self, cmd: App<'a, 'b>) -> App<'a, 'b> {
    cmd.about("Builds the project's targets")
  }

  fn run(&self, _ctx: &Context) -> RunResult {
    Ok(())
  }
}

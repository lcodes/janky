use clap::{App};

use crate::ctx::{Command, Context, RunResult};

pub struct Check;

impl Command for Check {
  fn init<'a, 'b>(&self, cmd: App<'a, 'b>) -> App<'a, 'b> {
    cmd.about("Checks whether the project's configuration is valid")
  }

  fn run(&self, _ctx: &Context) -> RunResult {
    Ok(())
  }
}

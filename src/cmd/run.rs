use clap::{App};

use crate::ctx::{Command, Context, RunResult};

pub struct Run;

impl Command for Run {
  fn init<'a, 'b>(&self, cmd: App<'a, 'b>) -> App<'a, 'b> {
    cmd.about("Runs the project's main executable")
  }

  fn run(&self, _ctx: &Context) -> RunResult {
    Ok(())
  }
}

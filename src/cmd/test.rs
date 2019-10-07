use clap::{App};

use crate::ctx::{Command, Context, RunResult};

pub struct Test;

impl Command for Test {
  fn init<'a, 'b>(&self, cmd: App<'a, 'b>) -> App<'a, 'b> {
    cmd.about("Runs the project's test suite")
  }

  fn run(&self, _ctx: &Context) -> RunResult {
    Ok(())
  }
}

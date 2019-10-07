use clap::{App};

use crate::ctx::{Command, Context, RunResult};

pub struct Show;

impl Command for Show {
  fn init<'a, 'b>(&self, cmd: App<'a, 'b>) -> App<'a, 'b> {
    cmd.about("Displays information")
  }

  fn run(&self, _ctx: &Context) -> RunResult {
    Ok(())
  }
}

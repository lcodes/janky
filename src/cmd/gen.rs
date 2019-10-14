use clap::{App};

use crate::ctx::{Command, Context, RunResult};

pub struct Gen;

impl Command for Gen {
  fn init<'a, 'b>(&self, cmd: App<'a, 'b>) -> App<'a, 'b> {
    cmd.about("Generates the project's build files")
  }

  fn run(&self, ctx: &Context) -> RunResult {
    #[cfg(unix)]
    for (_, g) in &ctx.generators {
      g.run(ctx)?;
    }
    // TODO get all generators to work on windows
    #[cfg(windows)]
    ctx.generators["vs"].run(ctx)?;
    Ok(())
  }
}

// NOTE: Tried to parallelize run() using crossbeam_utils::thread::scoped,
//       it ended up being ~20ms slower in release builds.
//       May want to try again later with larger projects, and when
//       generators get more complex.

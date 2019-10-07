mod build;
mod check;
mod gen;
mod run;
mod show;
mod test;

use crate::ctx::Commands;

pub fn init() -> Commands {
  let mut commands = Commands::new();
  commands.insert("build", Box::new(build::Build));
  commands.insert("check", Box::new(check::Check));
  commands.insert("gen",   Box::new(gen::Gen));
  commands.insert("run",   Box::new(run::Run));
  commands.insert("show",  Box::new(show::Show));
  commands.insert("test",  Box::new(test::Test));
  commands
}

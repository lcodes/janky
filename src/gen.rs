mod cmake;
mod gradle;
mod make;
mod vs;
mod xcode;

use crate::ctx::Generators;

pub fn init() -> Generators {
  let mut generators = Generators::new();
  generators.insert("cmake",  Box::new(cmake::CMake));
  generators.insert("gradle", Box::new(gradle::Gradle));
  generators.insert("make",   Box::new(make::Make));
  generators.insert("vs",     Box::new(vs::VisualStudio));
  generators.insert("xcode",  Box::new(xcode::XCode));
  generators
}

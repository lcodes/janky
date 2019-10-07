mod android;
mod ios;
mod linux;
mod macos;
mod tvos;
mod windows;

use crate::ctx::Platforms;

pub fn init() -> Platforms {
  let platforms: Platforms = vec!(
    Box::new(windows::Windows),
    Box::new(linux::Linux),
    Box::new(macos::MacOS),
    Box::new(ios::IOS),
    Box::new(tvos::TVOS),
    Box::new(android::Android)
  );

  for (i, p) in platforms.iter().enumerate() {
    let t = p.get_platform_type();
    assert!(t as usize == i, "Platform type mismatch for {:?}: got {} but expected {}",
            t, t as usize, i);
  }

  platforms
}

mod android;
mod ios;
mod html5;
mod linux;
mod macos;
mod tvos;
mod watchos;
mod windows;

use crate::ctx::Platforms;

pub fn init() -> Platforms {
  let platforms: Platforms = vec!(
    Box::new(windows::Windows),
    Box::new(linux::Linux),
    Box::new(macos::MacOS),
    Box::new(ios::IOS),
    Box::new(tvos::TVOS),
    Box::new(watchos::WatchOS),
    Box::new(android::Android),
    Box::new(html5::HTML5)
  );

  for (i, p) in platforms.iter().enumerate() {
    let t = p.get_platform_type();
    assert!(t as usize == i, "Platform type mismatch for {:?}: got {} but expected {}",
            t, t as usize, i);
  }

  platforms
}

use crate::ctx::{Context, Generator, PlatformType, RunResult};

pub struct CMake;

impl Generator for CMake {
  fn supports_platform(&self, p: PlatformType) -> bool {
    match p {
      PlatformType::Any     => unreachable!(),
      PlatformType::Android => true,
      PlatformType::Linux   => false,
      PlatformType::MacOS   => false,
      PlatformType::IOS     => false,
      PlatformType::TVOS    => false,
      PlatformType::Windows => false
    }
  }

  fn run(&self, _ctx: &Context) -> RunResult {
    Ok(())
  }
}

// CMakeLists.txt

/*

cmake_minimum_required(VERSION 3.10.2)
project({} LANGUAGES CXX)

set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -Wall -Werror -fnoexceptions -fno-rtti")

add_library({} SHARED AndroidMain.cpp)
set_target_properties({}
  PROPERTIES
  CXX_STANDARD 11
  CXX_STANDARD_REQUIRED YES
  CXX_EXTENSIONS NO)

# https://github.com/android-ndk/ndk/issues/381
set_target_properties({}
  PROPERTIES
  LINK_FLAGS "-u ANativeActivity_onCreate")

target_link_libraries({}
  PRIVATE
  ...)
*/

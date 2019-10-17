use std::fs::{File, create_dir_all};
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::ctx::{Context, Generator, PlatformType, RunResult, Target, TargetType};

pub struct Gradle;

impl Generator for Gradle {
  fn supports_platform(&self, p: PlatformType) -> bool {
    match p {
      PlatformType::Any     => unreachable!(),
      PlatformType::Android => true,
      _                     => false
    }
  }

  fn run(&self, ctx: &Context) -> RunResult {
    if !ctx.project.filter.matches_platform(PlatformType::Android) {
      return Ok(());
    }

    let targets = ctx.project.targets.iter().enumerate().filter_map(|(index, (name, target))| {
      match target.filter.matches_platform(PlatformType::Android) &&
        target.target_type == TargetType::Application {
          false => None,
          true  => Some(Build { name, target, index, path: [name, "_Android"].join("") })
        }}).collect::<Vec<Build>>();

    if targets.is_empty() {
      return Ok(());
    }

    for build in &targets {
      write_target_build(ctx, build)?;
    }

    write_root_build(&ctx)?;
    write_properties(&ctx)?;
    write_settings(ctx, &targets)?;

    Ok(())
  }
}

type IO = std::io::Result<()>;

struct Build<'a> {
  path:   String,
  name:   &'a str,
  target: &'a Target<'a>,
  index:  usize
}

fn write_target_build(ctx: &Context, build: &Build) -> IO {
  let mut path = ctx.build_dir.join(&build.path);
  create_dir_all(&path)?;

  let mut f = BufWriter::new(File::create(path.join("build.gradle"))?);

  write!(f, concat!("apply plugin: 'com.android.application'\n\n",
                    "android {{\n",
                    "  compileSdkVersion {compile_sdk_version}\n",
                    "  buildToolsVersion '{build_tools_version}'\n\n",
                    "  defaultConfig {{\n",
                    "    applicationId '{application_id}'\n",
                    "    minSdkVersion {min_sdk_version}\n",
                    "    targetSdkVersion {target_sdk_version}\n",
                    "    versionCode {version_code}\n",
                    "    versionName '{version_name}'\n\n",
                    "    ndk.abiFilters 'arm64-v8a'\n\n", // TODO dont hardcode filters
                    "    sourceSets {{\n",
                    "      main {{\n",
                    "        manifest.srcFile 'AndroidManifest.xml'\n",
                    "        res.srcDirs = ['res']\n", // TODO place assets there
                    "      }}\n",
                    "    }}\n",
                    "  }}\n\n",
                    "  externalNativeBuild {{\n",
                    "    cmake {{\n",
                    "      version '{cmake_version}'\n",
                    "      path 'CMakeLists.txt'\n",
                    "    }}\n",
                    "  }}\n\n",
                    "  buildTypes {{\n"),
         // TODO dont hardcode
         compile_sdk_version = 29,
         build_tools_version = "29.0.2",
         application_id      = "com.lambdacoder.Jank",
         version_code        = 1,
         version_name        = "1.0",
         min_sdk_version     = 26,
         target_sdk_version  = 29,
         cmake_version       = "3.10.2")?;

  for &prof in &ctx.profiles {
    write!(f, "    {} {{\n", prof.to_lowercase())?;

    match prof { // TODO dont hardcode this
      "Debug" => {
        f.write_all(concat!("      packagingOptions {\n",
                            "        doNotStrip '**.so'\n",
                            "      }\n").as_bytes())?;
      },
      "Release" => {
        f.write_all(concat!("      minifyEnabled true\n",
                            "      proguardFiles getDefaultProguardFile('proguard-android.txt'),",
                            " 'proguard-rules.pro'\n").as_bytes())?;
      },
      _ => {}
    }

    f.write_all(b"    }\n")?;
  }

  f.write_all(b"  }\n")?;

  // TODO productFlavors
  // TODO buildVariants
  // TODO manifest entries
  // TODO signing
  // TODO splits
  // TODO lintOptions

  f.write_all(b"}\n")?;

  // TODO dependencies

  // TODO handle assets
  // - AndroidManifest.xml
  // - symlink other xml files
  write_target_manifest(ctx, &path, build)?;

  f.flush()?;
  Ok(())
}

fn write_root_build(ctx: &Context) -> IO {
  let mut f = File::create(ctx.build_dir.join("build.gradle"))?;
  f.write_all(concat!("buildscript {\n",
                      "  repositories {\n",
                      "    google()\n",
                      "    jcenter()\n",
                      "  }\n\n",
                      "  dependencies {\n",
                      "    classpath 'com.android.tools.build:gradle:3.5.0'\n",
                      "  }\n",
                      "}\n\n",
                      "allprojects {\n",
                      "  repositories {\n",
                      "    google()\n",
                      "    jcenter()\n",
                      "  }\n",
                      "}\n\n",
                      "task clean(type: Delete) {\n",
                      "  delete rootProject.buildDir\n",
                      "}\n").as_bytes())?;

  f.flush()?;
  Ok(())
}

fn write_properties(ctx: &Context) -> IO {
  let mut f = File::create(ctx.build_dir.join("gradle.properties"))?;
  f.write_all(b"org.gradle.jvmargs=-Xmx8g\n")?;
  Ok(())
}

fn write_settings(ctx: &Context, builds: &[Build]) -> IO {
  let mut f = BufWriter::new(File::create(ctx.build_dir.join("settings.gradle"))?);
  f.write_all(b"include ")?;

  let mut iter = builds.iter();
  write!(f, "':{}'", iter.next().unwrap().path)?;

  for build in iter {
    write!(f, ", ':{}'", build.path)?;
  }

  f.write_all(b"\n")?;
  f.flush()?;
  Ok(())
}

const XML_DECL: &[u8] = b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n";

/// https://developer.android.com/guide/topics/manifest/manifest-intro
fn write_target_manifest(ctx: &Context, path: &Path, build: &Build) -> IO {
  // TODO android TV banner

  // TODO uses-configuration
  // TODO uses-library
  // TODO uses-permission / uses-permission-sdk-23
  // TODO supports-gl-texture
  // TODO supports-screens

  // TODO dont hardcode
  let features = ["android.hardware.audio.output",
                  "android.hardware.screen.landscape"];
  let feature_versions = [("android.hardware.vulkan.compute", "0"),
                          ("android.hardware.vulkan.level",   "0"),
                          ("android.hardware.vulkan.version", "0x400003")];

  let mut f = BufWriter::new(File::create(path.join("AndroidManifest.xml"))?);
  f.write_all(XML_DECL)?;

  write!(f, concat!("<manifest\n",
                    "    xmlns:android=\"http://schemas.android.com/apk/res/android\"\n",
                    "    package=\"{application_id}\"\n",
                    "    android:versionCode=\"{version_code}\"\n",
                    "    android:versionName=\"{version_name}\">\n",
                    "  <uses-sdk\n",
                    "      android:minSdkVersion=\"{min_sdk_version}\"\n",
                    "      android:targetSdkVersion=\"{target_sdk_version}\" />\n"),
         application_id     = "com.lambdacoder.Jank",
         version_code       = 1,
         version_name       = "1.0",
         min_sdk_version    = 26,
         target_sdk_version = 29)?;

  for name in &features { // TODO android:required attribute
    write!(f, "  <uses-feature android:name=\"{}\" />\n", name)?;
  }

  write!(f, "  <uses-feature android:name=\"android.hardware.touchscreen\" android:required=\"false\" />")?;

  for (name, version) in &feature_versions {
    write!(f, concat!("  <uses-feature\n",
                      "      android:name=\"{name}\"\n",
                      "      android:version=\"{version}\"\n",
                      "      android:required=\"true\" />\n"),
           name    = name,
           version = version)?;
  }

  // TODO android:name ?
  write!(f, concat!("  <application\n",
                    "      android:allowBackup=\"false\"\n",
                    "      android:description=\"@string/app_description\"\n",
                    "      android:label=\"@string/app_label\"\n",
                    "      android:icon=\"@mipmap/ic_launcher\"\n",
                    "      android:roundIcon=\"@mipmap/ic_launcher_round\"\n",
                    // "      android:theme=\"@style/AppTheme\"\n",
                    "      android:isGame=\"true\"\n",
                    "      android:hasCode=\"false\">\n",
                    "    <activity\n",
                    "        android:name=\"android.app.NativeActivity\"\n",
                    "        android:configChanges=\"{config_changes}\">\n",
                    "      <meta-data\n",
                    "          android:name=\"android.app.lib_name\"\n",
                    "          android:value=\"{target_name}\" />\n",
                    "      <intent-filter>\n",
                    "        <action android:name=\"android.intent.action.MAIN\" />\n",
                    "        <category android:name=\"android.intent.category.LAUNCHER\" />\n",
                    "      </intent-filter>\n",
                    "    </activity>\n",
                    "  </application>\n",
                    "</manifest>\n"),
         // TODO dont hardcode
         target_name        = build.name,
         config_changes     = "keyboardHidden|keyboard|orientation|screenSize")?;

  write_strings(ctx, path)?;
  write_mipmaps(ctx, path, build)?;
  // - styles

  f.flush()?;
  Ok(())
}

fn write_strings(ctx: &Context, path: &Path) -> IO {
  let mut res = path.join("res/values");
  create_dir_all(&res)?;
  res.push("string.xml");

  let mut f = BufWriter::new(File::create(res)?);
  f.write_all(XML_DECL)?;
  f.write_all(b"<resources>\n")?;

  // TODO more strings? TODO from target, not project
  let strings = [("app_label",       ctx.project.name),
                 ("app_description", ctx.project.description)];

  for (name, value) in &strings {
    write!(f, "  <string name=\"{}\">{}</string>\n", name, value)?;
  }

  f.write_all(b"</resources>\n")?;
  f.flush()?;
  Ok(())
}

fn write_mipmaps(ctx: &Context, path: &Path, build: &Build) -> IO {
  if build.target.assets.is_none() {
    return Ok(());
  }

  let src = pathdiff::diff_paths(&ctx.input_dir, &path.join("res/mipmap")).unwrap();

  let pattern = [build.target.assets.unwrap(), "/android/"].join("");
  let assets  = ctx.assets[build.index].iter()
    .filter(|info| info.meta.is_file() && info.to_str().starts_with(&pattern));

  for asset in assets {
    let s = &asset.to_str()[pattern.len() ..];

    if !s.ends_with(".png") {
      continue;
    }

    if let Some(pos) = s.rfind('_') {
      let dpi  = &s[pos + 1 .. s.len() - 4];
      let name = &s[0 .. pos];

      let mut res = path.join(["res/mipmap-", dpi].join(""));
      create_dir_all(&res)?;

      res.push([name, ".png"].join(""));
      // TODO move remove&symlink to shared utility
      if res.symlink_metadata().is_ok() {
        std::fs::remove_file(&res)?;
      }

      #[cfg(unix)]
      std::os::unix::fs::symlink(src.join(&asset.path), &res)?;

      // TODO
      // #[cfg(windows)]
      // std::os::windows::fs::symlink_file(src.join(&asset.path), &res)?;
    }
  }

  let adaptive_path = path.join("res/mipmap-anydpi-v26");
  create_dir_all(&adaptive_path)?;

  let background = "@mipmap/ic_launcher_background"; // TODO color/vector backgrounds
  let foreground = "@mipmap/ic_launcher_foreground";

  write_adaptive_icon(&adaptive_path.join("ic_launcher.xml"),       background, foreground)?;
  write_adaptive_icon(&adaptive_path.join("ic_launcher_round.xml"), background, foreground)?;

  Ok(())
}

fn write_adaptive_icon(path: &Path, background: &str, foreground: &str) -> IO {
  let mut f = File::create(path)?;
  f.write_all(XML_DECL)?;

  write!(f, concat!("<adaptive-icon xmlns:android=\"http://schemas.android.com/apk/res/android\">\n",
                    "  <background android:drawable=\"{}\" />\n",
                    "  <foreground android:drawable=\"{}\" />\n",
                    "</adaptive-icon>\n"),
         background, foreground)?;

  f.flush()?;
  Ok(())
}

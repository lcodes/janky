use std::fs::{File, create_dir_all};
use std::io::{BufWriter, Write};

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

    let targets = ctx.project.targets.iter().filter_map(|(name, target)| {
      match target.filter.matches_platform(PlatformType::Android) &&
        target.target_type == TargetType::Application {
          false => None,
          true  => Some(Build { name, target, path: [name, "_Android"].join("") })
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
  target: &'a Target<'a>
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
        f.write(concat!("      packagingOptions {\n",
                        "        doNotStrip '**.so'\n",
                        "      }\n").as_bytes())?;
      },
      "Release" => {
        f.write(concat!("      minifyEnabled true\n",
                        "      proguardFiles getDefaultProguardFile('proguard-android.txt'),",
                        " 'proguard-rules.pro'\n").as_bytes())?;
      },
      _ => {}
    }

    f.write(b"    }\n")?;
  }

  f.write(b"  }\n")?;

  // TODO productFlavors
  // TODO buildVariants
  // TODO manifest entries
  // TODO signing
  // TODO splits
  // TODO lintOptions

  f.write(b"}\n")?;

  // TODO dependencies

  // TODO handle assets
  // - AndroidManifest.xml
  // - symlink other xml files
  write_target_manifest(ctx, build, &path)?;

  f.flush()?;
  Ok(())
}

fn write_root_build(ctx: &Context) -> IO {
  let mut f = File::create(ctx.build_dir.join("build.gradle"))?;
  f.write(concat!("buildscript {\n",
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
  f.write(b"org.gradle.jvmargs=-Xmx8g\n")?;
  Ok(())
}

fn write_settings(ctx: &Context, builds: &Vec<Build>) -> IO {
  let mut f = BufWriter::new(File::create(ctx.build_dir.join("settings.gradle"))?);
  f.write(b"include ")?;

  let mut iter = builds.iter();
  write!(f, "':{}'", iter.next().unwrap().path)?;

  for build in iter {
    write!(f, ", ':{}'", build.path)?;
  }

  f.write(b"\n")?;
  f.flush()?;
  Ok(())
}

fn write_target_manifest(ctx: &Context, build: &Build, path: &std::path::Path) -> IO {
  let mut f = File::create(path.join("AndroidManifest.xml"))?;
  write!(f, concat!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n",
                    "<manifest\n",
                    "    xmlns:android=\"http://schemas.android.com/apk/res/android\"\n",
                    "    package=\"{application_id}\"\n",
                    "    android:versionCode=\"{version_code}\"\n",
                    "    android:versionName=\"{version_name}\">\n",
                    "  <uses-sdk\n",
                    "      android:minSdkVersion=\"{min_sdk_version}\"\n",
                    "      android:targetSdkVersion=\"{target_sdk_version}\" />\n",
                    "  <application\n",
                    "      android:allowBackup=\"false\"\n",
                    "      android:fullBackupContent=\"false\"\n",
                    "      android:label=\"{target_name}\"\n",
                    // "      android:theme=\"@style/AppTheme\"\n",
                    "      android:hasCode=\"false\">\n",
                    "    <activity\n",
                    "        android:name=\"android.app.NativeActivity\"\n",
                    "        android:label=\"{target_name}\"\n",
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
         application_id     = "com.lambdacoder.Jank",
         version_code       = 1,
         version_name       = "1.0",
         min_sdk_version    = 26,
         target_sdk_version = 29,
         target_name        = build.name,
         config_changes     = "keyboardHidden|keyboard|orientation|screenSize"
  )?;

  f.flush()?;
  Ok(())
}

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use uuid::Uuid;

use crate::ctx::{Architecture, Context, Generator, PlatformType, RunResult, Target};

pub struct VisualStudio;

impl Generator for VisualStudio {
  fn supports_platform(&self, p: PlatformType) -> bool {
    match p {
      PlatformType::Any     => unreachable!(),
      PlatformType::Windows => true,
      PlatformType::Android => true,
      PlatformType::Linux   => false,
      PlatformType::IOS     => false,
      PlatformType::MacOS   => false,
      PlatformType::TVOS    => false
    }
  }

  fn run(&self, ctx: &Context) -> RunResult {
    let tools = Tools::new(Version::VS2019); // TODO configure
    let projs: Vec<Proj> = ctx.project.targets
      .iter()
      .map(|kv| Proj::new(kv, &ctx.build_dir))
      .collect();

    for (i, proj) in projs.iter().enumerate() {
      write_proj(ctx, &tools, i, proj)?;
    }
    write_sln(ctx, &tools, &projs)?;
    Ok(())
  }
}

type IO = std::io::Result<()>;

#[derive(Clone, Copy)]
enum Version {
  VS2015,
  VS2017,
  VS2019
}

struct Tools {
  version:       Version,
  version_major: &'static str,
  version_extra: &'static str,
  xmlns:         &'static str
}

impl Tools {
  fn new(version: Version) -> Self {
    let version_major = match version {
      Version::VS2015 => "14",
      Version::VS2017 => "15",
      Version::VS2019 => "16"
    };
    let version_extra = match version {
      Version::VS2015 => "0.23107.0",
      Version::VS2017 => "2.26430.4",
      Version::VS2019 => "0.28729.10"
    };
    Tools {
      version,
      version_major,
      version_extra,
      xmlns: "http://schemas.microsoft.com/developer/msbuild/2003"
    }
  }
}

fn random_uuid() -> String {
  Uuid::new_v4().to_string().to_uppercase()
}

enum ProjKind {
  Android,
  CXX
}

struct Proj<'a> {
  target:    &'a Target<'a>,
  name:      &'a str,
  kind:      ProjKind,
  path:      PathBuf,
  uuid:      String,
  is_folder: bool
}

impl<'a> Proj<'a> {
  fn new((name, target): (&&'a str, &'a Target<'a>), build_dir: &PathBuf) -> Self {
    let kind = ProjKind::CXX;
    let path = build_dir.join(name).with_extension(match kind {
      ProjKind::Android => "androidproj",
      ProjKind::CXX     => "vcxproj"
    });

    Proj {
      target, name, kind, path,
      uuid:      random_uuid(),
      is_folder: false
    }
  }

  fn get_kind_guid(&self) -> &str {
    if self.is_folder {
      "2150E333-8FDC-42A3-9474-1A3956D46DE8"
    }
    else {
      match self.kind {
        ProjKind::Android => "39E2626F-3545-4960-A6E8-258AD8476CE5",
        ProjKind::CXX     => "8BC9CEB8-8B4A-11D0-8D11-00A0C91BC942"
      }
    }
  }

  fn get_platform_toolset(&self, v: Version) -> &'static str {
    match self.kind {
      ProjKind::Android => "Clang_5_0",
      ProjKind::CXX     => match v {
        Version::VS2015 => "", // TODO
        Version::VS2017 => "v141",
        Version::VS2019 => "v142"
      }
    }
  }
}

fn write_proj(ctx: &Context, tools: &Tools, index: usize, proj: &Proj) -> IO {
  let mut f = File::create(ctx.build_dir.join(&ctx.project.name).with_extension("vcxproj"))?;

  write!(f, concat!(r#"<?xml version="1.0" encoding="utf-8"?>"#, "\r\n",
                    r#"<Project DefaultTargets="Build""#,
                    r#" ToolsVersion="{}.0" xmlns="{}">"#, ">\r\n"),
         tools.version_major,
         tools.xmlns)?;

  f.write_all(b"  <ItemGroup Label=\"ProjectConfigurations\">\r\n")?;
  /*
  for prof in &ctx.project.profiles {
    for arch in &ctx.project.architectures {
      write!(f, concat!("    <ProjectConfiguration Include=\"{0}|{1}\">\r\n",
                        "       <Configuration>{0}</Configuration>\r\n",
                        "       <Platform>{1}</Platform>\r\n",
                        "    </ProjectConfiguration>\r\n"),
             prof.name, get_arch_name(*arch))?;
    }
  }*/
  f.write_all(b"  </ItemGroup>\r\n")?;

  f.write_all(b"  <PropertyGroup Label=\"Globals\">\r\n")?;
  write!(f, "    <ProjectGuid>{{{}}}</ProjectGuid>\r\n", proj.uuid)?;
  //f.write_fmt(format_args!("    <Keyword>{}</Keyword>\r\n", "Android"))?;
  write!(f, "    <RootNamespace>{}</RootNamespace>\r\n", proj.name)?;
  f.write_all(b"  </PropertyGroup>\r\n")?;

  write_proj_import(&mut f, match proj.kind {
    ProjKind::Android => r#"$(AndroidTargetsPath)\Android.Default.props"#,
    ProjKind::CXX     => r#"$(VCTargetsPath)\Microsoft.Cpp.Default.props"#
  })?;

  // TODO per profile/architecture configurations

  write_proj_import(&mut f, match proj.kind {
    ProjKind::Android => r#"$(AndroidTargetsPath)\Android.props"#,
    ProjKind::CXX     => r#"$(VCTargetsPath)\Microsoft.Cpp.props"#
  })?;
  f.write_all(b"  <ImportGroup Label=\"ExtensionSettings\" />\r\n")?;
  f.write_all(b"  <ImportGroup Label=\"Shared\" />\r\n")?;

  // TODO import property sheets

  f.write_all(b"  <PropertyGroup Label=\"UserMacros\" />\r\n")?;

  // TODO item definition groups
  // TODO pch
  // TODO link dependencies

  // TODO project references

  let files = &ctx.files[index];
  match proj.kind {
    ProjKind::Android => {

    },
    ProjKind::CXX => {
      // write_files(&mut f, "ClInclude", files.iter().filter(|f| f.extension().unwrap_or_default() == "h"))?;
      // write_files(&mut f, "ClCompile", files.iter().filter(|f| f.extension().unwrap_or_default() == "cpp"))?;
    }
  }

  write_proj_import(&mut f, match proj.kind {
    ProjKind::Android => r#"$(AndroidTargetsPath)\Android.targets"#,
    ProjKind::CXX     => r#"$(VCTargetsPath)\Microsoft.Cpp.Targets"#
  })?;
  f.write_all(b"  <ImportGroup Label=\"ExtensionTargets\" />\r\n")?;

  f.write_all(b"</Project>\r\n")?;
  f.flush()?;
  Ok(())
}

fn write_files<'a, I>(f: &mut File, elem: &str, files: I) -> IO where
  I: std::iter::Iterator<Item = &'a PathBuf>
{
  f.write_all(b"  <ItemGroup>\r\n")?;
  for file in files {
    write!(f, "    <{} Include=\"{}\" />\r\n", elem, file.to_str().unwrap())?;
  }
  f.write_all(b"  </ItemGroup>\r\n")?;
  Ok(())
}

fn write_sln(ctx: &Context, tools: &Tools, projs: &[Proj]) -> IO {
  let mut f = File::create(ctx.build_dir.join(&ctx.project.name).with_extension("sln"))?;

  f.write_all(b"\xEF\xBB\xBF\r\n")?;
  write!(f, concat!("Microsoft Visual Studio Solution File, Format Version 12.00\r\n",
                    "# Visual Studio Version {0}\r\n",
                    "VisualStudioVersion = {0}.{1}\r\n",
                    "MinimumVisualStudioVersion = {0}.{1}\r\n"),
         tools.version_major,
         tools.version_extra)?;

  for proj in projs {
    write!(f, concat!(r#"Project("{{{}}}") = "{}", "{}", "{{{}}}""#, "\r\n"),
           proj.get_kind_guid(),
           proj.name,
           proj.path.to_str().unwrap(),
           proj.uuid)?;
    for _dep in &proj.target.depends {

    }
    f.write_all(b"EndProject\r\n")?;
  }

  f.write_all(b"Global\r\n")?;

  f.write_all(b"  GlobalSection(SolutionConfigurationPlatforms) = preSolution\r\n")?;
  /*for prof in &ctx.project.profiles {
    for arch in &ctx.project.architectures {
      write!(f, "    {0}|{1} = {0}{1}\r\n", prof.name, get_arch_name(*arch))?;
    }
  }*/
  f.write_all(b"  EndGlobalSection\r\n")?;

  f.write_all(b"  GlobalSection(ProjectConfigurationPlatforms) = preSolution\r\n")?;
  /*for proj in projs {
    for prof in &ctx.project.profiles {
      for arch in &ctx.project.architectures {
        let arch_name = get_arch_name(*arch);
        // TODO dont enable all 3 for everything
        write_sln_config(&mut f, &proj.uuid, &prof.name, arch_name, "ActiveCfg")?;
        write_sln_config(&mut f, &proj.uuid, &prof.name, arch_name, "Build.0")?;
        write_sln_config(&mut f, &proj.uuid, &prof.name, arch_name, "Deploy.0")?;
      }
    }
  }*/
  f.write_all(b"  EndGlobalSection\r\n")?;

  f.write_all(b"  GlobalSection(SolutionProperties) = preSolution\r\n")?;
  f.write_all(b"    HideSolutionNode = FALSE\r\n")?;
  f.write_all(b"  EndGlobalSection\r\n")?;

  f.write_all(b"  GlobalSection(NestedProjects) = preSolution\r\n")?;
  // TODO folders
  f.write_all(b"  EndGlobalSection\r\n")?;

  f.write_all(b"  GlobalSection(ExtensibilityGlobals) = postSolution\r\n")?;
  write!(f, "    SolutionGuid = {{{}}}\r\n", random_uuid())?;
  f.write_all(b"  EndGlobalSection\r\n")?;

  f.write_all(b"EndGlobal\r\n")?;
  f.flush()?;
  Ok(())
}

fn write_proj_import(f: &mut File, v: &str) -> IO {
  write!(f, "  <Import Project=\"{}\" />\r\n", v)
}

fn write_sln_config(f: &mut File, uuid: &str, prof: &str, arch: &str, action: &str) -> IO {
  write!(f, "    {{{0}}}.{1}|{2}.{3} = {1}|{2}\r\n", uuid, prof, arch, action)
}

fn get_arch_name(arch: Architecture) -> &'static str {
  match arch {
    Architecture::Any   => unreachable!(),
    Architecture::X86   => "Win32",
    Architecture::X64   => "x64",
    Architecture::ARM   => "ARM",
    Architecture::ARM64 => "ARM64"
  }
}

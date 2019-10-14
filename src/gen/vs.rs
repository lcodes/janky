use std::collections::HashSet;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::ctx::{Architecture, Context, Generator, FileInfo, PlatformType, RunResult, Target};

pub struct VisualStudio;

impl Generator for VisualStudio {
  fn supports_platform(&self, p: PlatformType) -> bool {
    match p {
      PlatformType::Any     => unreachable!(),
      PlatformType::Windows => true,
      PlatformType::Android => true,
      _                     => false
    }
  }

  fn run(&self, ctx: &Context) -> RunResult {
    let tools = Tools::new(Version::VS2019); // TODO configure
    let projs = ctx.project.targets
      .iter()
      .map(|kv| Proj::new(kv, &ctx.build_dir))
      .collect::<Vec<Proj>>();

    for (i, proj) in projs.iter().enumerate() {
      write_proj(ctx, &tools, i, proj)?;
      write_filters(ctx, &tools, i, proj)?;
    }
    write_sln(ctx, &tools, &projs)?;
    Ok(())
  }
}

type IO = std::io::Result<()>;

const ARCHITECTURES: &[Architecture] = &[ // TODO derive from project
  // Architecture::ARM, // TODO only when using the android toolchain
  // Architecture::ARM64,
  Architecture::X86,
  Architecture::X64
];

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

  fn write_file_header<W>(&self, f: &mut W) -> IO where W: Write {
    write!(f, concat!(r#"<?xml version="1.0" encoding="utf-8"?>"#, "\r\n",
                      "<Project xmlns=\"{}\"",
                      // r#"<Project DefaultTargets="Build""#,
                      // r#" ToolsVersion="{}.0" xmlns="{}""#,
                      ">\r\n"),
           // self.version_major,
           self.xmlns)?;
    Ok(())
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
  fn new((name, target): (&&'a str, &'a Target<'a>), build_dir: &Path) -> Self {
    let kind = ProjKind::CXX; // TODO
    let mut path = build_dir.join(name);
    path.set_extension(match kind {
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

fn get_item_group_element(file: &FileInfo) -> &'static str {
  // TODO more types (ie image)
  match file.extension() {
    "h" | "hpp" => "ClInclude",
    "c" | "cpp" => "ClCompile",
    "xml"       => "Xml",
    _           => "Text"
  }
}

fn write_filter_dir<'a, W>(f: &mut W, set: &mut HashSet<&'a Path>, path: &'a Path) -> IO where W: Write {
  if let Some(p) = path.parent() {
    if !p.to_str().unwrap().is_empty() && !set.contains(p) {
      set.insert(p);
      write_filter_dir(f, set, p)?;
    }
  }

  write!(f, concat!("    <Filter Include=\"{dir}\">\r\n",
                    "      <UniqueIdentifier>{{{uuid}}}</UniqueIdentifier>\r\n",
                    "    </Filter>\r\n"),
         dir  = path.to_str().unwrap(),
         uuid = random_uuid())?;
  Ok(())
}

fn write_filters(ctx: &Context, tools: &Tools, index: usize, proj: &Proj) -> IO {
  let mut f = BufWriter::new(File::create(proj.path.with_extension("vcxproj.filters"))?); // TODO androidproj ?
  tools.write_file_header(&mut f)?;
  f.write_all(b"  <ItemGroup>\r\n")?;

  let mut dir_set = HashSet::new();

  let files = &ctx.sources[index];
  for dir in files.iter().filter(|x| x.meta.is_dir()) {
    write_filter_dir(&mut f, &mut dir_set, &dir.path)?;
  }

  f.write_all(concat!("  </ItemGroup>\r\n",
                      "  <ItemGroup>\r\n").as_bytes())?;

  let prefix = ctx.input_rel.to_str().unwrap();
  for file in files.iter().filter(|x| x.meta.is_file()) {
    if let Some(filter) = file.path.parent() {
      write!(f, concat!("    <{element} Include=\"{prefix}\\{include}\">\r\n",
                        "      <Filter>{filter}</Filter>\r\n",
                        "    </{element}>\r\n"),
             element = get_item_group_element(file),
             prefix  = prefix,
             include = file.to_str(),
             filter  = filter.to_str().unwrap())?;
    }
  }

  f.write_all(concat!("  </ItemGroup>\r\n",
                      "</Project>\r\n").as_bytes())?;

  f.flush()?;
  Ok(())
}

fn write_proj(ctx: &Context, tools: &Tools, index: usize, proj: &Proj) -> IO {
  let mut f = BufWriter::new(File::create(&proj.path)?);
  tools.write_file_header(&mut f)?;

  f.write_all(b"  <ItemGroup Label=\"ProjectConfigurations\">\r\n")?;
  for arch in ARCHITECTURES {
    for prof in &ctx.profiles {
      write!(f, concat!("    <ProjectConfiguration Include=\"{profile}|{platform}\">\r\n",
                        "       <Configuration>{profile}</Configuration>\r\n",
                        "       <Platform>{platform}</Platform>\r\n",
                        "    </ProjectConfiguration>\r\n"),
             profile  = prof,
             platform = get_arch_platform(*arch))?;
    }
  }
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

  write!(f, concat!("  <PropertyGroup Label=\"Configuration\">\r\n",
                    "    <ConfigurationType>{config_type}</ConfigurationType>\r\n",
                    "    <PlatformToolset>{toolset}</PlatformToolset>\r\n",
                    "    <CharacterSet>Unicode</CharacterSet>\r\n",
                    "  </PropertyGroup>\r\n"),
         // TODO
         config_type = "Application",
         toolset     = "v142")?;

  // TODO hardcoded
  for prof in &ctx.profiles {
    write!(f, concat!("  <PropertyGroup Condition=\"'$(Configuration)'=='{profile}'\"",
                      " Label=\"Configuration\">\r\n",
                      "    <UseDebugLibraries>{debug:?}</UseDebugLibraries>\r\n"),
           profile = prof,
           debug   = *prof != "Release")?;

    if *prof == "Release" {
      f.write_all(b"  <WholeProgramOptimization>true</WholeProgramOptimization>\r\n")?;
    }

    f.write_all(b"  </PropertyGroup>\r\n")?;
  }

  write_proj_import(&mut f, match proj.kind {
    ProjKind::Android => r#"$(AndroidTargetsPath)\Android.props"#,
    ProjKind::CXX     => r#"$(VCTargetsPath)\Microsoft.Cpp.props"#
  })?;
  f.write_all(b"  <ImportGroup Label=\"ExtensionSettings\">\r\n  </ImportGroup>\r\n")?;
  f.write_all(b"  <ImportGroup Label=\"Shared\">\r\n  </ImportGroup>\r\n")?;

  write!(f, concat!("  <ImportGroup Label=\"PropertySheets\">\r\n",
                    "    <Import Project=\"{path}\" Condition=\"exists('{path}')\" />\r\n",
                    "  </ImportGroup>\r\n"),
         path = "$(UserRootDir)\\Microsoft.Cpp.$(Platform).user.props")?;

  f.write_all(b"  <PropertyGroup Label=\"UserMacros\" />\r\n")?;

  f.write_all(concat!("  <PropertyGroup>\r\n",
                      "    <GenerateManifest>false</GenerateManifest>\r\n",
                      "  </PropertyGroup>\r\n").as_bytes())?;

  // TODO general properties for profiles/architectures

  // TODO includes
  write!(f, concat!("  <ItemDefinitionGroup>\r\n",
                    "    <ClCompile>\r\n",
                    "      <WarningLevel>EnableAllWarnings</WarningLevel>\r\n",
                    "      <SDLCheck>true</SDLCheck>\r\n",
                    "      <ConformanceMode>true</ConformanceMode>\r\n",
                    "      <MultiProcessorCompilation>true</MultiProcessorCompilation>\r\n",
                    "      <LanguageStandard>stdcpp17</LanguageStandard>\r\n",
                    "      <RuntimeTypeInfo>false</RuntimeTypeInfo>\r\n",
                    // TODO disable exceptions
                    "      <CompileAsManaged>false</CompileAsManaged>\r\n",
                    "      <EnableEnhancedInstructionSet>AdvancedVectorExtensions2</EnableEnhancedInstructionSet>\r\n",
                    "    </ClCompile>\r\n",
                    "    <Link>\r\n",
                    "      <SubSystem>{subsystem}</SubSystem>",
                    "    </Link>\r\n",
                    "  </ItemDefinitionGroup>\r\n"),
         subsystem = "Windows")?;

  // TODO hardcoded
  for prof in &ctx.profiles {
    write!(f, concat!("  <ItemDefinitionGroup Condition=\"'$(Configuration)'=='{profile}'\">\r\n",
                      "    <ClCompile>\r\n",
                      "      <Optimization>{optimization}</Optimization>\r\n"),
           profile      = prof,
           optimization = match *prof == "Release" {
             true  => "MaxSpeed",
             false => "None"
           })?;

    if *prof == "Release" {
      f.write_all(concat!("      <FunctionLevelLinking>true</FunctionLevelLinking>\r\n",
                          "      <IntrinsicFunctions>true</IntrinsicFunctions>\r\n").as_bytes())?;
    }

    f.write_all(b"    </ClCompile>\r\n")?;
    // TODO link dependencies

    f.write_all(b"  </ItemDefinitionGroup>\r\n")?;
  }

  // TODO project references

  // TODO per file settings? (at least create PCH)
  let files = &ctx.sources[index];
  let prefix = ctx.input_rel.to_str().unwrap();
  f.write_all(b"  <ItemGroup>\r\n")?;
  match proj.kind {
    ProjKind::Android => {

    },
    ProjKind::CXX => {
      for file in files.iter().filter(|x| x.meta.is_file()) {
        write!(f, "    <{} Include=\"{}\\{}\" />\r\n",
               get_item_group_element(file), prefix, file.to_str())?;
      }
    }
  }
  f.write_all(b"  </ItemGroup>\r\n")?;

  // TODO resources
  // - resources.rc
  // - icon.ico
  // - manifest.xml

  write_proj_import(&mut f, match proj.kind {
    ProjKind::Android => r#"$(AndroidTargetsPath)\Android.targets"#,
    ProjKind::CXX     => r#"$(VCTargetsPath)\Microsoft.Cpp.Targets"#
  })?;
  f.write_all(b"  <ImportGroup Label=\"ExtensionTargets\" />\r\n")?;

  // TODO extensions? (ie PIX)
  // TODO nuget?

  f.write_all(b"</Project>\r\n")?;
  f.flush()?;
  Ok(())
}

fn write_sln(ctx: &Context, tools: &Tools, projs: &[Proj]) -> IO {
  let mut f = BufWriter::new(File::create({
    let mut path = ctx.build_dir.join(&ctx.project.name);
    path.set_extension("sln");
    path
  })?);

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
           proj.path.file_name().unwrap().to_str().unwrap(),
           proj.uuid)?;
    for dep in &proj.target.depends {
      // TODO
    }
    f.write_all(b"EndProject\r\n")?;
  }

  f.write_all(b"Global\r\n")?;

  f.write_all(b"  GlobalSection(SolutionConfigurationPlatforms) = preSolution\r\n")?;
  for prof in &ctx.profiles {
    for arch in ARCHITECTURES {
      write!(f, "    {0}|{1} = {0}|{1}\r\n", prof, get_arch_name(*arch))?;
    }
  }
  f.write_all(b"  EndGlobalSection\r\n")?;

  f.write_all(b"  GlobalSection(ProjectConfigurationPlatforms) = postSolution\r\n")?;
  for proj in projs {
    for prof in &ctx.profiles {
      for arch in ARCHITECTURES {
        // TODO dont enable all 3 for everything
        write_sln_config(&mut f, &proj.uuid, &prof, *arch, "ActiveCfg")?;
        write_sln_config(&mut f, &proj.uuid, &prof, *arch, "Build.0")?;
        // write_sln_config(&mut f, &proj.uuid, &prof, *arch, "Deploy.0")?;
      }
    }
  }
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

fn write_proj_import<W>(f: &mut W, v: &str) -> IO where W: Write {
  write!(f, "  <Import Project=\"{}\" />\r\n", v)
}

fn write_sln_config<W>(f: &mut W, uuid: &str, prof: &str, arch: Architecture,
                       action: &str) -> IO where W: Write
{
  write!(f, "    {{{uuid}}}.{profile}|{arch}.{action} = {profile}|{platform}\r\n",
         uuid     = uuid,
         action   = action,
         profile  = prof,
         arch     = get_arch_name(arch),
         platform = get_arch_platform(arch))
}

fn get_arch_name(arch: Architecture) -> &'static str {
  match arch {
    Architecture::Any   => unreachable!(),
    Architecture::ARM   => "ARM",
    Architecture::ARM64 => "ARM64",
    Architecture::X86   => "x86",
    Architecture::X64   => "x64"
  }
}

fn get_arch_platform(arch: Architecture) -> &'static str {
  match arch {
    Architecture::Any   => unreachable!(),
    Architecture::ARM   => "ARM",
    Architecture::ARM64 => "ARM64",
    Architecture::X86   => "Win32",
    Architecture::X64   => "x64"
  }
}

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufWriter, Result as IOResult, Write};
use std::path::Path;
use uuid::Uuid;

use crate::ctx::{Architecture, Context, Generator, FileInfo,
                 PlatformType, RunResult, Target, TargetFiles};

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
    let     tools = Tools::new(Version::VS2019); // TODO configure
    let mut projs = Vec::with_capacity(ctx.project.targets.len() + 1);

    projs.push(Proj {
      kind:   ProjKind::Items,
      uuid:   random_uuid(),
      name:   ctx.project.name,
      target: None
    });

    projs.extend(ctx.project.targets.iter().map(|(name, target)| { Proj {
      kind:   ProjKind::CXX,
      uuid:   random_uuid(),
      name:   name,
      target: Some(target)
    }}));

    for (i, proj) in projs.iter().skip(1).enumerate() {
      write_proj   (ctx, i, proj, &tools)?;
      write_filters(ctx, i, proj)?;
    }

    write_items(ctx, &projs[0])?;
    write_sln  (ctx, &projs, &tools)?;
    Ok(())
  }
}

type IO = IOResult<()>;

const ARCHITECTURES: &[Architecture] = &[ // TODO derive from project
  // Architecture::ARM, // TODO only when using the android toolchain
  // Architecture::ARM64,
  // Architecture::X86, // TODO keep generated GUIDs across generations
                        //      to prevent user selections from resetting
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
  version_extra: &'static str
}

impl Tools {
  fn new(version: Version) -> Self {
    Tools {
      version,
      version_major: match version {
        Version::VS2015 => "14",
        Version::VS2017 => "15",
        Version::VS2019 => "16"
      },
      version_extra: match version {
        Version::VS2015 => "0.23107.0",
        Version::VS2017 => "2.26430.4",
        Version::VS2019 => "0.28729.10"
      }
    }
  }
}

#[derive(PartialEq)]
enum ProjKind {
  Android,
  CXX,
  Items
}

struct Proj<'a> {
  kind:   ProjKind,
  uuid:   String,
  name:   &'a str,
  target: Option<&'a Target<'a>>
}

impl<'a> Proj<'a> {
  fn ext(&self) -> &'static str {
    match self.kind {
      ProjKind::Android => "androidproj",
      ProjKind::CXX     => "vcxproj",
      ProjKind::Items   => "vcxitems"
    }
  }

  fn create(&self, base: &Path, ext: &str) -> IOResult<BufWriter<File>> {
    let mut path = base.join(self.name);
    path.set_extension(ext);

    let mut f = BufWriter::new(File::create(&path)?);
    f.write_all(concat!(
      "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n",
      "<Project xmlns=\"http://schemas.microsoft.com/developer/msbuild/2003\">\r\n"
    ).as_bytes())?;

    Ok(f)
  }

  fn get_kind_guid(&self) -> &str {
    // TODO use solution folders? GUID = "2150E333-8FDC-42A3-9474-1A3956D46DE8"
    match self.kind {
      ProjKind::Android => "39E2626F-3545-4960-A6E8-258AD8476CE5",
      ProjKind::Items   |
      ProjKind::CXX     => "8BC9CEB8-8B4A-11D0-8D11-00A0C91BC942"
    }
  }

  fn get_platform_toolset(&self, v: Version) -> &'static str {
    match self.kind {
      ProjKind::Android => "Clang_5_0",
      ProjKind::CXX     => match v {
        Version::VS2015 => "", // TODO
        Version::VS2017 => "v141",
        Version::VS2019 => "v142"
      },
      ProjKind::Items   => unreachable!()
    }
  }
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

fn get_item_group_element(target: &Target, file: &FileInfo) -> &'static str {
  if !target.match_file(&file.path, PlatformType::Windows) {
    return "None";
  }

  // TODO more types (ie image)
  match file.extension() {
    "h" | "hpp" => "ClInclude",
    "c" | "cpp" => "ClCompile",
    "xml"       => "Xml",
    _           => "None"
  }
}

fn random_uuid() -> String {
  Uuid::new_v4().to_string().to_uppercase()
}


// Filter File
// -----------------------------------------------------------------------------

fn write_filters(ctx: &Context, index: usize, proj: &Proj) -> IO {
  assert!(proj.kind == ProjKind::CXX);

  let mut f = proj.create(&ctx.build_dir, "vcxproj.filters")?;
  f.write_all(b"  <ItemGroup>\r\n")?;

  let files = &ctx.sources[index];
  {
    let mut dir_set = HashSet::new();
    for &extend_index in &ctx.extends[index] {
      write_filter_dirs(&mut f, &mut dir_set, &ctx.sources[extend_index])?;
    }
    write_filter_dirs(&mut f, &mut dir_set, files)?;
  }

  f.write_all(concat!("  </ItemGroup>\r\n",
                      "  <ItemGroup>\r\n").as_bytes())?;

  let prefix = ctx.input_rel.to_str().unwrap();
  for &extend_index in &ctx.extends[index] {
    write_filter_files(&mut f, prefix, &ctx.sources[extend_index],
                       ctx.get_target(extend_index))?;
  }
  write_filter_files(&mut f, prefix, files, proj.target.unwrap())?;

  f.write_all(concat!("  </ItemGroup>\r\n",
                      "</Project>\r\n").as_bytes())?;

  f.flush()?;
  Ok(())
}

fn write_filter_dirs<'a, W>(f:     &mut W,
                            set:   &mut HashSet<&'a Path>,
                            files: &'a TargetFiles) -> IO where W: Write
{
  for dir in files.iter().filter(|x| x.meta.is_dir()) {
    write_filter_dir(f, set, &dir.path)?;
  }
  Ok(())
}

fn write_filter_dir<'a, W>(f:    &mut W,
                           set:  &mut HashSet<&'a Path>,
                           path: &'a Path) -> IO where W: Write
{
  if let Some(p) = path.parent() {
    // FIXME: better way to test empty path than getting a string slice?
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

fn write_filter_files<W>(f: &mut W, prefix: &str, files: &TargetFiles,
                         target: &Target) -> IO where W: Write
{
  for file in files.iter().filter(|x| x.meta.is_file()) {
    if let Some(filter) = file.path.parent() {
      write!(f, concat!("    <{element} Include=\"{prefix}\\{include}\">\r\n",
                        "      <Filter>{filter}</Filter>\r\n",
                        "    </{element}>\r\n"),
             element = get_item_group_element(target, file),
             prefix  = prefix,
             include = file.to_str(),
             filter  = filter.to_str().unwrap())?;
    }
  }
  Ok(())
}


// C++ Project File
// -----------------------------------------------------------------------------

fn write_proj(ctx: &Context, index: usize, proj: &Proj, tools: &Tools) -> IO {
  let mut f = proj.create(&ctx.build_dir, proj.ext())?;

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
  write!(f, concat!("    <RootNamespace>{project_name}</RootNamespace>\r\n",
                    "    <OutDir>$(Platform)\\$(Configuration)\\{project_name}\\</OutDir>\r\n",
                    "    <IntDir>$(Platform)\\$(Configuration)\\{project_name}\\</IntDir>\r\n"),
         project_name = proj.name)?;
  f.write_all(b"  </PropertyGroup>\r\n")?;

  write_proj_import(&mut f, match proj.kind {
    ProjKind::Android => r#"$(AndroidTargetsPath)\Android.Default.props"#,
    ProjKind::CXX     => r#"$(VCTargetsPath)\Microsoft.Cpp.Default.props"#,
    ProjKind::Items   => unreachable!()
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
    ProjKind::CXX     => r#"$(VCTargetsPath)\Microsoft.Cpp.props"#,
    ProjKind::Items   => unreachable!()
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

  let disable_warnings = "4324;4514;4571;4623;4625;4626;4710;4820;5026;5027;5045;6031;6387;26444";
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
                    "      <DisableSpecificWarnings>{warnings}</DisableSpecificWarnings>\r\n"),
         warnings = disable_warnings)?;

  let prefix = ctx.input_rel.to_str().unwrap();
  let target = proj.target.unwrap();

  write!(f, concat!("      <EnableEnhancedInstructionSet>AdvancedVectorExtensions2</EnableEnhancedInstructionSet>\r\n",
                    "      <AdditionalOptions>/experimental:preprocessor %(AdditionalOptions)</AdditionalOptions>\r\n",
                    "    </ClCompile>\r\n",
                    "    <Link>\r\n",
                    "      <SubSystem>{subsystem}</SubSystem>\r\n",
                    "    </Link>\r\n",
                    "  </ItemDefinitionGroup>\r\n"),
         subsystem = "Windows")?;

  // TODO hardcoded
  for prof in &ctx.profiles {
    let prof_lc = prof.to_lowercase();

    write!(f, concat!("  <ItemDefinitionGroup Condition=\"'$(Configuration)'=='{profile}'\">\r\n",
                      "    <ClCompile>\r\n",
                      "      <Optimization>{optimization}</Optimization>\r\n"),
           profile      = prof,
           optimization = match *prof == "Release" {
             true  => "MaxSpeed",
             false => "Disabled"
           })?;

    if *prof == "Release" {
      f.write_all(concat!("      <FunctionLevelLinking>true</FunctionLevelLinking>\r\n",
                          "      <IntrinsicFunctions>true</IntrinsicFunctions>\r\n").as_bytes())?;
    }

    write!(f, "      <AdditionalIncludeDirectories>{}\\3rdparty\\include\\{};", prefix, prof_lc)?;

    for &extend_index in &ctx.extends[index] {
      write_includes(&mut f, prefix, ctx.get_target(extend_index))?;
    }
    write_includes(&mut f, prefix, target)?;

    f.write_all(concat!("%(AdditionalIncludeDirectories)</AdditionalIncludeDirectories>\r\n",
                        "      <PreprocessorDefinitions>").as_bytes())?;

    if *prof == "Debug" {
      f.write_all(b"_ITERATOR_DEBUG_LEVEL=1;")?;
    }
    for &extend_index in &ctx.extends[index] {
      write_defines(&mut f, ctx.get_target(extend_index))?;
    }
    write_defines(&mut f, target)?;

    f.write_all(concat!("%(PreprocessorDefinitions)</PreprocessorDefinitions>\r\n",
                        "    </ClCompile>\r\n",
                        "    <Link>\r\n",
                        "      <AdditionalDependencies>").as_bytes())?;

    // TODO hardcoded
    f.write_all(b"OpenGL32.lib;")?;
    for &extend_index in &ctx.extends[index] {
      for lib in &*ctx.get_target(extend_index).settings.libs {
        write!(f, "{}.lib;", lib)?;
      }
    }
    for lib in &*target.settings.libs {
      write!(f, "{}.lib;", lib)?;
    }

    f.write_all(concat!("%(AdditionalDependencies)</AdditionalDependencies>\r\n",
                        "      <AdditionalLibraryDirectories>").as_bytes())?;

    write!(f, "{}\\3rdparty\\lib\\windows\\x64\\{}", prefix, prof_lc)?;

    f.write_all(concat!("</AdditionalLibraryDirectories>\r\n",
                        "    </Link>\r\n",
                        "  </ItemDefinitionGroup>\r\n").as_bytes())?;
  }

  // TODO project references

  // TODO per file settings? (at least create PCH)
  f.write_all(b"  <ItemGroup>\r\n")?;
  match proj.kind {
    ProjKind::Android => {

    },
    ProjKind::CXX => {
      for &extend_index in &ctx.extends[index] {
        write_files(&mut f, ctx, extend_index, prefix, ctx.get_target(extend_index))?;
      }
      write_files(&mut f, ctx, index, prefix, target)?;
    },
    ProjKind::Items => unreachable!()
  }
  f.write_all(b"  </ItemGroup>\r\n")?;

  // TODO resources
  // - resources.rc
  // - icon.ico
  // - manifest.xml

  write_proj_import(&mut f, match proj.kind {
    ProjKind::Android => r#"$(AndroidTargetsPath)\Android.targets"#,
    ProjKind::CXX     => r#"$(VCTargetsPath)\Microsoft.Cpp.Targets"#,
    ProjKind::Items   => unreachable!()
  })?;
  f.write_all(b"  <ImportGroup Label=\"ExtensionTargets\" />\r\n")?;

  // TODO extensions? (ie PIX)
  // TODO nuget?

  f.write_all(b"</Project>\r\n")?;
  f.flush()?;
  Ok(())
}

fn write_includes<W>(f: &mut W, prefix: &str, target: &Target) -> IO where W: Write {
  for inc in &*target.settings.include_dirs {
    write!(f, "{}\\{};", prefix, inc.replace("/", "\\"))?;
  }
  Ok(())
}

fn write_defines<W>(f: &mut W, target: &Target) -> IO where W: Write {
  for def in &*target.settings.defines {
    write!(f, "{};", def)?;
  }
  Ok(())
}

fn write_files<W>(f: &mut W, ctx: &Context, index: usize,
                  prefix: &str, target: &Target) -> IO where W: Write
{
  for file in ctx.sources[index].iter().filter(|x| x.meta.is_file()) {
    write!(f, "    <{} Include=\"{}\\{}\" />\r\n",
           get_item_group_element(target, file), prefix, file.to_str())?;
  }

  Ok(())
}


// Items Project File
// -----------------------------------------------------------------------------

fn write_items(ctx: &Context, proj: &Proj) -> IO {
  let mut f = proj.create(&ctx.build_dir, proj.ext())?;
  write!(f, concat!("  <PropertyGroup Label=\"Globals\">\r\n",
                    "    <ItemsProjectGuid>{{{}}}</ItemsProjectGuid>\r\n",
                    "  </PropertyGroup>\r\n",
                    "  <ItemGroup>\r\n"),
         proj.uuid)?;

  let path = ctx.input_rel.to_str().unwrap();
  for file in ctx.metafiles.iter().filter(|x| x.meta.is_file()) {
    write!(f, "    <None Include=\"$(MSBuildThisFileDirectory){}\\{}\" />\r\n",
           path, file.name())?;
  }

  f.write_all(concat!("  </ItemGroup>\r\n",
                      "</Project>\r\n").as_bytes())?;
  f.flush()?;
  Ok(())
}


// Solution File
// -----------------------------------------------------------------------------

fn write_sln(ctx: &Context, projs: &[Proj], tools: &Tools) -> IO {
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

  let path = ctx.build_dir.to_str().unwrap();
  for proj in projs {
    write!(f, concat!(r#"Project("{{{kind}}}") = "{name}", "#,
                      r#""{path}\\{name}.{ext}", "{{{uuid}}}""#, "\r\n"),
           kind = proj.get_kind_guid(),
           path = path,
           name = proj.name,
           ext  = proj.ext(),
           uuid = proj.uuid)?;

    if let Some(target) = proj.target {
      for dep in &target.depends {
        // TODO
      }
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

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Text;
using System.Xml;
using Microsoft.Build.Construction;
using Microsoft.Build.Evaluation;
using Microsoft.Build.Exceptions;
using Microsoft.Build.Framework;

namespace Tethys.MSBuild.Evaluate;

internal static class Evaluation
{
    private static readonly string[] PropertyNames = (
        "TargetFramework TargetFrameworks TargetFrameworkIdentifier TargetFrameworkVersion TargetFrameworkProfile TargetFrameworkMoniker " +
        "TargetPlatformIdentifier TargetPlatformVersion Platform PlatformTarget Configuration AssemblyName RootNamespace " +
        "DefineConstants LangVersion Nullable AllowUnsafeBlocks OutputType OutputPath TargetPath TargetFileName " +
        "MSBuildProjectFullPath MSBuildProjectDirectory MSBuildProjectExtensionsPath ProjectAssetsFile RestoreProjectStyle " +
        "NuGetPackageRoot RestorePackagesPath RestorePackagesConfig RestoreRepositoryPath RestoreConfigFile ManagePackageVersionsCentrally " +
        "BaseIntermediateOutputPath IntermediateOutputPath RuntimeIdentifier RuntimeIdentifiers " +
        "VSToolsPath UsingMicrosoftNETSdk MSBuildToolsVersion MSBuildVersion MSBuildFileVersion NETCoreSdkVersion MSBuildRuntimeType MSBuildBinPath"
    ).Split(' ');
    private static readonly string[] BuiltInMetadata = (
        "FullPath RootDir Filename Extension RelativeDir Directory RecursiveDir Identity " +
        "DefiningProjectFullPath DefiningProjectDirectory DefiningProjectName DefiningProjectExtension"
    ).Split(' ');

    // This boundary must not be inlined into the method registering Locator.
    [MethodImpl(MethodImplOptions.NoInlining)]
    public static Response Run(Request request)
    {
        var response = new Response { project_path = request.project_path };
        var logger = new EvaluationLogger(response.diagnostics);
        try
        {
            var assembly = typeof(ProjectCollection).Assembly;
            var actualPath = Path.GetDirectoryName(assembly.Location)!;
            response.host = new Host
            {
#if NETFRAMEWORK
                kind = "framework",
                runtime = ".NET Framework CLR " + Environment.Version,
#else
                kind = "sdk",
                runtime = System.Runtime.InteropServices.RuntimeInformation.FrameworkDescription,
#endif
                path = actualPath,
                version = ProjectCollection.Version.ToString()
            };
            var comparison = Path.DirectorySeparatorChar == '\\' ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;
            if (!string.Equals(Path.GetFullPath(actualPath).TrimEnd(Path.DirectorySeparatorChar),
                request.msbuild_path.TrimEnd(Path.DirectorySeparatorChar), comparison))
                throw new InvalidOperationException("Loaded MSBuild assembly is not from the selected installation.");
            var globals = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            foreach (var entry in request.global_properties)
            {
                if (entry.Value == null) throw new ArgumentException("Global property values must be strings.");
                globals.Add(entry.Key, entry.Value);
            }
            if (request.target_framework != null)
            {
                if (request.target_framework.Length == 0) throw new ArgumentException("Use null, not an empty framework selector.");
                if (globals.TryGetValue("TargetFramework", out var explicitFramework) && explicitFramework != request.target_framework)
                    throw new ArgumentException("Target framework selector conflicts with explicit TargetFramework global property.");
                globals["TargetFramework"] = request.target_framework;
            }
            using var collection = new ProjectCollection(globals);
            collection.IsBuildEnabled = false;
            collection.RegisterLogger(logger);
            var project = collection.LoadProject(request.project_path);
            foreach (var name in PropertyNames.Concat(globals.Keys).Distinct(StringComparer.OrdinalIgnoreCase))
                response.properties[name] = project.GetPropertyValue(name);
            // The pre-load check proves which assembly was loaded, not which toolset it
            // resolves: an externally hosted AnyCPU/x64 process can be routed to an amd64
            // sibling (plan.md:206). Refuse to certify metadata from another toolset.
            var toolset = project.GetPropertyValue("MSBuildBinPath");
            if (string.IsNullOrWhiteSpace(toolset)
                || !string.Equals(Path.GetFullPath(toolset).TrimEnd(Path.DirectorySeparatorChar),
                    Path.GetFullPath(request.msbuild_path).TrimEnd(Path.DirectorySeparatorChar), comparison))
                throw new InvalidOperationException("Effective MSBuild toolset is not the selected installation.");
            foreach (var kind in response.items.Keys)
            {
                foreach (var evaluated in project.GetItems(kind))
                {
                    var item = new Item { include = evaluated.EvaluatedInclude, full_path = evaluated.GetMetadataValue("FullPath") };
                    foreach (var metadata in evaluated.Metadata)
                        item.metadata[metadata.Name] = metadata.EvaluatedValue;
                    foreach (var name in BuiltInMetadata)
                        item.metadata[name] = evaluated.GetMetadataValue(name);
                    response.items[kind].Add(item);
                }
            }
            var imports = new HashSet<string>(Path.DirectorySeparatorChar == '\\' ? StringComparer.OrdinalIgnoreCase : StringComparer.Ordinal);
            foreach (var import in project.Imports)
                if (imports.Add(import.ImportedProject.FullPath)) response.imports.Add(import.ImportedProject.FullPath);
            // Logical XML preserves the originating file and original patterns, including Remove
            // and false-condition item declarations. Re-expanding against final properties would
            // corrupt evidence when a property was reassigned after an item declaration.
            foreach (var element in project.GetLogicalProject())
            {
                if (element is not ProjectItemElement item || item.AllParents.Any(parent => parent is ProjectTargetElement)) continue;
                response.glob_patterns.Add(new GlobPattern
                {
                    project_path = item.ContainingProject.FullPath, item_type = item.ItemType,
                    include = item.Include, exclude = item.Exclude, remove = item.Remove
                });
            }
            QualifyLiteralRecipe(project, response);
            if (logger.Overflowed) throw new IOException("Evaluation diagnostics exceed 1 MiB.");
            response.success = !response.diagnostics.Any(diagnostic => diagnostic.severity == "error");
            response.cache_eligible = response.success && response.cache_ineligibility.Count == 0;
            if (!response.success) response.cache_ineligibility.Add("evaluation_failed");
        }
        catch (InvalidProjectFileException error)
        {
            logger.Add(new Diagnostic
            {
                code = error.ErrorCode, message = error.BaseMessage, file = error.ProjectFile,
                line = error.LineNumber, column = error.ColumnNumber, exception_type = error.GetType().FullName
            });
            response.cache_ineligibility.Add("evaluation_failed");
        }
        catch (Exception error)
        {
            logger.Add(new Diagnostic { message = error.Message, exception_type = error.GetType().FullName });
            response.cache_ineligibility.Add("evaluation_failed");
        }
        if (logger.Overflowed)
        {
            response.success = false;
            response.cache_eligible = false;
            response.diagnostics.Clear();
            response.diagnostics.Add(new Diagnostic { message = "Evaluation diagnostics exceed 1 MiB.", exception_type = typeof(IOException).FullName });
            response.cache_ineligibility.Add("diagnostics_overflow");
        }
        return response;
    }

    private static void QualifyLiteralRecipe(Project project, Response response)
    {
        // This is an intentionally closed recipe, not a purity claim about arbitrary MSBuild.
        // No import (even a false/optional import), SDK, condition, expression or unknown
        // construction is reusable. S3 must still key the project, source/glob inventory,
        // globals, environment and exact host, and verify inputs before/after evaluation.
        if (project.Imports.Count != 0) response.cache_ineligibility.Add("unqualified_import_closure");
        var document = new XmlDocument { XmlResolver = null };
        using var text = new StringReader(project.Xml.RawXml);
        using var reader = XmlReader.Create(text, new XmlReaderSettings { DtdProcessing = DtdProcessing.Prohibit, XmlResolver = null });
        document.Load(reader);
        var root = document.DocumentElement!;
        var reasons = new HashSet<string>(StringComparer.Ordinal);
        Inspect(root, reasons);
        foreach (var reason in reasons.OrderBy(value => value, StringComparer.Ordinal))
            response.cache_ineligibility.Add(reason);
    }

    private static void Inspect(XmlElement element, HashSet<string> reasons)
    {
        var parent = element.ParentNode as XmlElement;
        var grandparent = parent?.ParentNode as XmlElement;
        bool root = parent == null && element.LocalName == "Project";
        bool group = parent?.LocalName == "Project" && (element.LocalName == "PropertyGroup" || element.LocalName == "ItemGroup");
        bool property = parent?.LocalName == "PropertyGroup" && grandparent?.LocalName == "Project";
        bool item = parent?.LocalName == "ItemGroup" && grandparent?.LocalName == "Project";
        bool metadata = grandparent?.LocalName == "ItemGroup" && grandparent.ParentNode is XmlElement owner && owner.LocalName == "Project";
        if (!(root || group || property || item || metadata)) reasons.Add("unqualified_project_construct");
        foreach (XmlAttribute attribute in element.Attributes)
        {
            if (attribute.Name == "xmlns" || attribute.Prefix == "xmlns") continue;
            if (attribute.LocalName == "Sdk") reasons.Add("unqualified_sdk_resolver");
            if (attribute.LocalName == "Condition") reasons.Add("unqualified_condition_dependencies");
            bool known = root && attribute.Name == "ToolsVersion" || item &&
                (attribute.Name == "Include" || attribute.Name == "Exclude" || attribute.Name == "Remove");
            if (!known) reasons.Add("unqualified_project_attribute");
            if (HasExpression(attribute.Value)) reasons.Add("unqualified_expression_dependencies");
        }
        foreach (XmlNode child in element.ChildNodes)
        {
            if (child is XmlElement nested) Inspect(nested, reasons);
            else if ((child is XmlText || child is XmlCDataSection) && HasExpression(child.Value ?? ""))
                reasons.Add("unqualified_expression_dependencies");
        }
    }

    private static bool HasExpression(string value) => value.Contains("$(") || value.Contains("@(") || value.Contains("%(");

    private sealed class EvaluationLogger(List<Diagnostic> diagnostics) : ILogger
    {
        private int bytes;
        public bool Overflowed { get; private set; }
        public LoggerVerbosity Verbosity { get; set; } = LoggerVerbosity.Normal;
        public string? Parameters { get; set; }
        public void Initialize(IEventSource eventSource)
        {
            eventSource.ErrorRaised += (_, error) => Add(new Diagnostic
            {
                code = error.Code, message = error.Message ?? "", file = error.File,
                line = error.LineNumber, column = error.ColumnNumber
            });
            eventSource.WarningRaised += (_, warning) => Add(new Diagnostic
            {
                code = warning.Code, message = warning.Message ?? "", file = warning.File,
                line = warning.LineNumber, column = warning.ColumnNumber, severity = "warning"
            });
        }
        public void Add(Diagnostic diagnostic)
        {
            if (Overflowed) return;
            // Include fixed record overhead and UTF-8 strings before retaining the record.
            long size = 128L + Encoding.UTF8.GetByteCount(diagnostic.message) +
                Encoding.UTF8.GetByteCount(diagnostic.code ?? "") + Encoding.UTF8.GetByteCount(diagnostic.file ?? "") +
                Encoding.UTF8.GetByteCount(diagnostic.exception_type ?? "");
            if (size > 1024 * 1024 - bytes) { Overflowed = true; return; }
            bytes += (int)size;
            diagnostics.Add(diagnostic);
        }
        public void Shutdown() { }
    }
}

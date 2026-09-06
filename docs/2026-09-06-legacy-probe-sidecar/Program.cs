// Throwaway probe: open a solution with Roslyn MSBuildWorkspace, report per-project
// load/compile/binding facts as JSON. No writes into the repository.
using System.Diagnostics;
using System.Text.Json;
using Microsoft.Build.Locator;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.MSBuild;

if (args.Length < 2) { Console.Error.WriteLine("usage: LegacyProbe <solution> <out.json> [Prop=Value ...]"); return 2; }
var slnPath = args[0];
var outPath = args[1];
var props = new Dictionary<string, string>();
for (int i = 2; i < args.Length; i++) { var kv = args[i].Split('=', 2); props[kv[0]] = kv.Length > 1 ? kv[1] : ""; }

var report = new Dictionary<string, object?>();
var instances = MSBuildLocator.QueryVisualStudioInstances().ToList();
report["msbuildInstances"] = instances.Select(i => $"{i.DiscoveryType} {i.Name} {i.Version} {i.MSBuildPath}").ToList();
var chosen = instances.OrderByDescending(i => i.DiscoveryType == DiscoveryType.VisualStudioSetup).ThenByDescending(i => i.Version).FirstOrDefault();
if (chosen != null) MSBuildLocator.RegisterInstance(chosen); else MSBuildLocator.RegisterDefaults();
report["msbuildChosen"] = chosen == null ? null : $"{chosen.DiscoveryType} {chosen.Name} {chosen.Version} {chosen.MSBuildPath}";
report["globalProperties"] = props;
report["os"] = Environment.OSVersion.ToString();
report["roslyn"] = typeof(Compilation).Assembly.GetName().Version?.ToString();

var failures = new List<object>();
using var ws = MSBuildWorkspace.Create(props);
ws.WorkspaceFailed += (_, e) => { lock (failures) failures.Add(new { kind = e.Diagnostic.Kind.ToString(), message = e.Diagnostic.Message }); };

var sw = Stopwatch.StartNew();
var solution = await ws.OpenSolutionAsync(slnPath);
report["solutionLoadSeconds"] = Math.Round(sw.Elapsed.TotalSeconds, 1);
report["workspaceFailures"] = failures;
report["workspaceFailureCounts"] = failures.GroupBy(f => ((dynamic)f).kind).ToDictionary(g => (string)g.Key, g => g.Count());

var allErrors = new List<object>();
var vbAssemblies = new HashSet<string>(solution.Projects.Where(p => p.Language == LanguageNames.VisualBasic).Select(p => p.AssemblyName));
var projects = new List<object>();
foreach (var p in solution.Projects)
{
    var psw = Stopwatch.StartNew();
    var row = new Dictionary<string, object?>
    {
        ["name"] = p.Name, ["language"] = p.Language, ["path"] = p.FilePath, ["assembly"] = p.AssemblyName,
        ["documents"] = p.Documents.Count(), ["metadataReferences"] = p.MetadataReferences.Count,
        ["projectReferences"] = p.ProjectReferences.Count(), ["analyzerReferences"] = p.AnalyzerReferences.Count,
        ["outputFilePath"] = p.OutputFilePath,
        ["preprocessorSymbols"] = p.ParseOptions is CSharpParseOptions cpo ? string.Join(";", cpo.PreprocessorSymbolNames) : null,
        ["metadataRefSample"] = p.MetadataReferences.Take(6).Select(r => Path.GetFileName(r.Display ?? "")).ToList(),
        ["metadataImageRefs"] = p.MetadataReferences.Count(r => r.GetType().Name.Contains("Image")),
    };
    try
    {
        var comp = await p.GetCompilationAsync();
        if (comp == null) { row["compilation"] = "null"; }
        else
        {
            var diags = comp.GetDiagnostics();
            var errs = diags.Where(d => d.Severity == DiagnosticSeverity.Error).ToList();
            row["errors"] = errs.Count;
            row["warnings"] = diags.Count(d => d.Severity == DiagnosticSeverity.Warning);
            row["topErrorIds"] = errs.GroupBy(d => d.Id).OrderByDescending(g => g.Count()).Take(8).ToDictionary(g => g.Key, g => g.Count());
            row["errorSamples"] = errs.Take(4).Select(d => d.ToString()).ToList();
            row["references"] = comp.References.Count();
            row["compilationRefs"] = comp.References.Count(r => r is CompilationReference);
            row["peRefs"] = comp.References.Count(r => r is PortableExecutableReference);
            var refAsm = new HashSet<string>(comp.ReferencedAssemblyNames.Select(a => a.Name));
            row["p2pMissingInCompilation"] = p.ProjectReferences
                .Select(pr => solution.GetProject(pr.ProjectId))
                .Where(rp => rp != null && !refAsm.Contains(rp!.AssemblyName))
                .Select(rp => rp!.Name + " (" + rp.Language + ")").ToList();
            lock (allErrors)
                foreach (var d in errs)
                    allErrors.Add(new { project = p.Name, id = d.Id, message = d.GetMessage().Length > 220 ? d.GetMessage()[..220] : d.GetMessage(), file = Path.GetFileName(d.Location.SourceTree?.FilePath ?? "") });
            if (p.Language == LanguageNames.CSharp)
            {
                int calls = 0, callsBound = 0, callsToVb = 0, callsMeta = 0, news = 0, newsBound = 0, members = 0, membersBound = 0;
                var cand = new Dictionary<string, int>();
                foreach (var tree in comp.SyntaxTrees)
                {
                    var model = comp.GetSemanticModel(tree);
                    foreach (var node in tree.GetRoot().DescendantNodes())
                    {
                        switch (node)
                        {
                            case InvocationExpressionSyntax inv:
                            {
                                calls++;
                                var si = model.GetSymbolInfo(inv);
                                if (si.Symbol != null)
                                {
                                    callsBound++;
                                    var asm = si.Symbol.ContainingAssembly?.Name;
                                    if (asm != null && vbAssemblies.Contains(asm)) callsToVb++;
                                    if (si.Symbol.Locations.Any(l => l.IsInMetadata)) callsMeta++;
                                }
                                else { var k = si.CandidateReason.ToString(); cand[k] = cand.GetValueOrDefault(k) + 1; }
                                break;
                            }
                            case ObjectCreationExpressionSyntax oc:
                                news++; if (model.GetSymbolInfo(oc).Symbol != null) newsBound++; break;
                            case MemberAccessExpressionSyntax ma when ma.Parent is not InvocationExpressionSyntax:
                                members++; if (model.GetSymbolInfo(ma).Symbol != null) membersBound++; break;
                        }
                    }
                }
                row["binding"] = new { calls, callsBound, callsToVbMetadata = callsToVb, callsToAnyMetadata = callsMeta, unboundCandidateReasons = cand, objectCreations = news, objectCreationsBound = newsBound, memberAccesses = members, memberAccessesBound = membersBound };
            }
        }
    }
    catch (Exception ex) { row["compilationException"] = ex.GetType().Name + ": " + ex.Message; }
    row["seconds"] = Math.Round(psw.Elapsed.TotalSeconds, 1);
    projects.Add(row);
    Console.Error.WriteLine($"{p.Name}: {row.GetValueOrDefault("errors")} errors in {row["seconds"]}s");
}
report["projects"] = projects;
report["totalSeconds"] = Math.Round(sw.Elapsed.TotalSeconds, 1);
report["peakWorkingSetMB"] = Process.GetCurrentProcess().PeakWorkingSet64 / (1024 * 1024);
File.WriteAllText(outPath, JsonSerializer.Serialize(report, new JsonSerializerOptions { WriteIndented = true }));
File.WriteAllLines(Path.ChangeExtension(outPath, ".errors.jsonl"), allErrors.Select(e => JsonSerializer.Serialize(e)));
Console.WriteLine($"wrote {outPath}: {projects.Count} projects, {failures.Count} workspace diagnostics, {report["totalSeconds"]}s");
return 0;

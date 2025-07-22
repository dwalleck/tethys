using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

namespace Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests;

public static class SourceGeneratorTestHelper
{
    public static Task<GeneratorDriverRunResult> RunGeneratorAsync<TGenerator>(string source, params string[] additionalSources)
        where TGenerator : IIncrementalGenerator, new()
    {
        var generator = new TGenerator();
        var driver = CSharpGeneratorDriver.Create(generator);

        var compilation = CreateCompilation(source, additionalSources);
        var updatedDriver = driver.RunGenerators(compilation);
        var result = updatedDriver.GetRunResult();

        return Task.FromResult(result);
    }

    public static CSharpCompilation CreateCompilation(string source, params string[] additionalSources)
    {
        var syntaxTrees = new List<SyntaxTree> { CSharpSyntaxTree.ParseText(source) };
        syntaxTrees.AddRange(additionalSources.Select(s => CSharpSyntaxTree.ParseText(s)));

        // Add IEndpoint interface
        var iEndpointSource = """
            namespace Stratify.MinimalEndpoints
            {
                public interface IEndpoint
                {
                    void MapEndpoint(Microsoft.AspNetCore.Routing.IEndpointRouteBuilder app);
                }
            }
            """;
        syntaxTrees.Add(CSharpSyntaxTree.ParseText(iEndpointSource));

        var references = GetStandardReferences();

        return CSharpCompilation.Create(
            "TestAssembly",
            syntaxTrees,
            references,
            new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));
    }

    public static List<MetadataReference> GetStandardReferences()
    {
        var references = new List<MetadataReference>();

        // Basic .NET references
        references.Add(MetadataReference.CreateFromFile(typeof(object).Assembly.Location));
        references.Add(MetadataReference.CreateFromFile(typeof(Enumerable).Assembly.Location));
        references.Add(MetadataReference.CreateFromFile(typeof(List<>).Assembly.Location));
        references.Add(MetadataReference.CreateFromFile(typeof(Task).Assembly.Location));
        references.Add(MetadataReference.CreateFromFile(typeof(Attribute).Assembly.Location));

        // ASP.NET Core references
        var aspNetCoreAssembly = AppDomain.CurrentDomain.GetAssemblies()
            .FirstOrDefault(a => a.GetName().Name == "Microsoft.AspNetCore.Http.Abstractions");
        if (aspNetCoreAssembly != null)
        {
            references.Add(MetadataReference.CreateFromFile(aspNetCoreAssembly.Location));
        }

        var routingAssembly = AppDomain.CurrentDomain.GetAssemblies()
            .FirstOrDefault(a => a.GetName().Name == "Microsoft.AspNetCore.Routing.Abstractions");
        if (routingAssembly != null)
        {
            references.Add(MetadataReference.CreateFromFile(routingAssembly.Location));
        }

        // Add some mock types if ASP.NET Core assemblies are not available
        if (aspNetCoreAssembly == null || routingAssembly == null)
        {
            var mockAspNetCore = """
                namespace Microsoft.AspNetCore.Routing
                {
                    public interface IEndpointRouteBuilder { }
                }
                namespace Microsoft.AspNetCore.Http
                {
                    public interface IResult { }
                    public static class Results
                    {
                        public static IResult Ok(object value) => null;
                        public static IResult BadRequest(object error) => null;
                        public static IResult StatusCode(int statusCode) => null;
                    }
                }
                namespace Microsoft.AspNetCore.Builder
                {
                    public class WebApplication { }
                }
                namespace Microsoft.Extensions.DependencyInjection
                {
                    public interface IServiceCollection { }
                }
                """;

            var mockTree = CSharpSyntaxTree.ParseText(mockAspNetCore);
            var mockCompilation = CSharpCompilation.Create(
                "MockAspNetCore",
                new[] { mockTree },
                references,
                new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));

            references.Add(mockCompilation.ToMetadataReference());
        }

        return references;
    }

    public static IEnumerable<SyntaxTree> GetGeneratedTrees(GeneratorDriverRunResult result)
    {
        return result.GeneratedTrees;
    }

    public static string? GetGeneratedContent(GeneratorDriverRunResult result, string fileName)
    {
        var tree = result.GeneratedTrees.FirstOrDefault(t => t.FilePath.EndsWith(fileName));
        return tree?.GetText().ToString();
    }

    public static bool HasErrors(GeneratorDriverRunResult result)
    {
        return result.Diagnostics.Any(d => d.Severity == DiagnosticSeverity.Error);
    }

    public static IEnumerable<Diagnostic> GetErrors(GeneratorDriverRunResult result)
    {
        return result.Diagnostics.Where(d => d.Severity == DiagnosticSeverity.Error);
    }
}

using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using TUnit.Core;
using System.Linq;
using TUnit.Assertions;
using TUnit.Assertions.Extensions;

namespace Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests;

public class DebugSnapshotTest
{
    [Test]
    public async Task Debug_Generator_Output()
    {
        var source = """
            using Stratify.MinimalEndpoints;

            namespace TestNamespace;

            [Endpoint(HttpMethod.Get, "/api/test")]
            public partial class TestEndpoint
            {
                public string Handle()
                {
                    return "Hello World";
                }
            }
            """;

        var attributeSource = """
            namespace Stratify.MinimalEndpoints
            {
                [System.AttributeUsage(System.AttributeTargets.Class)]
                public class EndpointAttribute : System.Attribute
                {
                    public string Pattern { get; }
                    public HttpMethod Method { get; }

                    public EndpointAttribute(HttpMethod method, string pattern)
                    {
                        Method = method;
                        Pattern = pattern;
                    }
                }

                public enum HttpMethod
                {
                    Get
                }
            }
            """;

        var syntaxTree = CSharpSyntaxTree.ParseText(source);
        var attributeSyntaxTree = CSharpSyntaxTree.ParseText(attributeSource);

        var references = new[]
        {
            MetadataReference.CreateFromFile(typeof(object).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(System.Attribute).Assembly.Location)
        };

        var compilation = CSharpCompilation.Create(
            assemblyName: "TestAssembly",
            syntaxTrees: new[] { attributeSyntaxTree, syntaxTree },
            references: references,
            options: new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));

        // Check compilation errors
        var compilationDiagnostics = compilation.GetDiagnostics();
        foreach (var diag in compilationDiagnostics.Where(d => d.Severity == DiagnosticSeverity.Error))
        {
            Console.WriteLine($"Compilation error: {diag}");
        }

        var generator = new EndpointGeneratorImproved();
        var driver = CSharpGeneratorDriver.Create(generator);

        driver = (CSharpGeneratorDriver)driver.RunGeneratorsAndUpdateCompilation(compilation, out var outputCompilation, out var diagnostics);

        var runResult = driver.GetRunResult();

        // Debug output
        Console.WriteLine($"Generator count: {runResult.Results.Length}");
        Console.WriteLine($"Diagnostics count: {diagnostics.Length}");

        foreach (var diagnostic in diagnostics)
        {
            Console.WriteLine($"Generator diagnostic: {diagnostic}");
        }

        var generatorResult = runResult.Results[0];
        Console.WriteLine($"Generated sources count: {generatorResult.GeneratedSources.Length}");

        foreach (var generatedSource in generatorResult.GeneratedSources)
        {
            Console.WriteLine($"Generated file: {generatedSource.HintName}");
            Console.WriteLine(generatedSource.SourceText.ToString());
        }

        // Also check if the attribute is found in the compilation
        var endpointClass = compilation.GetTypeByMetadataName("TestNamespace.TestEndpoint");
        Console.WriteLine($"Endpoint class found: {endpointClass != null}");

        if (endpointClass != null)
        {
            var attributes = endpointClass.GetAttributes();
            Console.WriteLine($"Attribute count: {attributes.Length}");
            foreach (var attr in attributes)
            {
                Console.WriteLine($"Attribute: {attr.AttributeClass?.ToDisplayString()}");
            }
        }

        await Assert.That(generatorResult.GeneratedSources.Length).IsGreaterThan(0);
    }
}

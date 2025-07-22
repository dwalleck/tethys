using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using TUnit.Core;
using System.Linq;
using TUnit.Assertions;
using TUnit.Assertions.Extensions;
using Stratify.MinimalEndpoints.ImprovedSourceGenerators;

namespace Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests;

public class SimpleDebugTest
{
    [Test]
    public async Task Simple_Debug_Test()
    {
        var source = """
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
                    Get,
                    Post,
                    Put,
                    Delete,
                    Patch,
                    Head,
                    Options
                }
            }

            namespace TestNamespace
            {
                using Stratify.MinimalEndpoints;

                [Endpoint(HttpMethod.Get, "/api/test")]
                public partial class TestEndpoint
                {
                    public string Handle()
                    {
                        return "Hello World";
                    }
                }
            }
            """;

        var syntaxTree = CSharpSyntaxTree.ParseText(source);

        var references = new[]
        {
            MetadataReference.CreateFromFile(typeof(object).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(System.Attribute).Assembly.Location)
        };

        var compilation = CSharpCompilation.Create(
            assemblyName: "TestAssembly",
            syntaxTrees: new[] { syntaxTree },
            references: references,
            options: new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));

        // Check compilation errors
        var compilationDiagnostics = compilation.GetDiagnostics();
        Console.WriteLine($"Compilation diagnostics: {compilationDiagnostics.Length}");
        foreach (var diag in compilationDiagnostics)
        {
            Console.WriteLine($"  {diag}");
        }

        // Check the class exists
        var testEndpointClass = compilation.GetTypeByMetadataName("TestNamespace.TestEndpoint");
        Console.WriteLine($"TestEndpoint class found: {testEndpointClass != null}");

        if (testEndpointClass != null)
        {
            var attributes = testEndpointClass.GetAttributes();
            Console.WriteLine($"TestEndpoint attribute count: {attributes.Length}");
            foreach (var attr in attributes)
            {
                Console.WriteLine($"  Attribute: {attr.AttributeClass?.ToDisplayString()}");
                Console.WriteLine($"  Constructor args: {attr.ConstructorArguments.Length}");
                for (int i = 0; i < attr.ConstructorArguments.Length; i++)
                {
                    var arg = attr.ConstructorArguments[i];
                    Console.WriteLine($"    Arg[{i}]: Kind={arg.Kind}, Type={arg.Type?.ToDisplayString()}, Value={arg.Value}");
                }
            }
        }

        var generator = new EndpointGeneratorImproved();
        var driver = CSharpGeneratorDriver.Create(generator);

        driver = (CSharpGeneratorDriver)driver.RunGeneratorsAndUpdateCompilation(compilation, out var outputCompilation, out var diagnostics);

        var runResult = driver.GetRunResult();

        Console.WriteLine($"Generator results: {runResult.Results.Length}");
        Console.WriteLine($"Generator diagnostics: {diagnostics.Length}");

        var generatorResult = runResult.Results[0];
        Console.WriteLine($"Generated sources: {generatorResult.GeneratedSources.Length}");

        foreach (var generatedSource in generatorResult.GeneratedSources)
        {
            Console.WriteLine($"Generated file: {generatedSource.HintName}");
            Console.WriteLine("Content:");
            Console.WriteLine(generatedSource.SourceText.ToString());
        }

        // Try to manually trace through the generation
        var endpointAttribute = compilation.GetTypeByMetadataName("Stratify.MinimalEndpoints.EndpointAttribute");
        Console.WriteLine($"\nEndpointAttribute type found: {endpointAttribute != null}");

        if (endpointAttribute != null)
        {
            // Find all classes with this attribute
            var classesWithAttribute = compilation.SyntaxTrees
                .SelectMany(tree => tree.GetRoot().DescendantNodes())
                .OfType<ClassDeclarationSyntax>()
                .Where(c => c.AttributeLists.Any(al => al.Attributes.Any(a =>
                {
                    var name = a.Name.ToString();
                    return name == "Endpoint" || name == "EndpointAttribute";
                })))
                .ToList();

            Console.WriteLine($"Classes with Endpoint attribute: {classesWithAttribute.Count}");
            foreach (var cls in classesWithAttribute)
            {
                Console.WriteLine($"  Class: {cls.Identifier.Text}");
                Console.WriteLine($"  Is partial: {cls.Modifiers.Any(m => m.IsKind(SyntaxKind.PartialKeyword))}");
            }
        }

        await Assert.That(generatorResult.GeneratedSources.Length).IsGreaterThan(0);
    }
}

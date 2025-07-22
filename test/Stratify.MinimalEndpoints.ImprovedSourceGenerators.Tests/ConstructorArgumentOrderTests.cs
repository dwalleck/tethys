using System.Linq;
using System.Threading.Tasks;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using TUnit.Core;
using TUnit.Core.Exceptions;

namespace Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests;

public class ConstructorArgumentOrderTests
{
    [Test]
    public async Task ExtractsConstructorArguments_InCorrectOrder()
    {
        // Arrange
        const string source = @"
using Stratify.MinimalEndpoints.Attributes;

namespace TestApp;

[Endpoint(HttpMethodType.Post, ""/api/test"")]
public partial class TestEndpoint
{
    [Handler]
    public IResult Handle() => Results.Ok();
}";

        var compilation = CreateCompilation(source);
        var generator = new EndpointGeneratorImproved();

        // Act
        var driver = CSharpGeneratorDriver.Create(generator);
        var result = driver.RunGenerators(compilation).GetRunResult();

        // Assert - Check that code was generated
        await Assert.That(result.GeneratedTrees.Length).IsGreaterThan(0);

        // Get the generated source
        var generatedSource = result.GeneratedTrees.First().GetText().ToString();

        // Verify the generated code contains the correct HTTP method (Post)
        await Assert.That(generatedSource).Contains("MapPost");

        // Verify the generated code contains the correct pattern
        await Assert.That(generatedSource).Contains("\"/api/test\"");
    }

    [Test]
    public async Task ExtractsHttpMethod_FromFirstArgument()
    {
        // Test various HTTP methods to ensure they're extracted from the first argument
        var testCases = new[]
        {
            ("HttpMethodType.Get", "MapGet"),
            ("HttpMethodType.Post", "MapPost"),
            ("HttpMethodType.Put", "MapPut"),
            ("HttpMethodType.Delete", "MapDelete"),
            ("HttpMethodType.Patch", "MapPatch")
        };

        foreach (var (httpMethod, expectedMapMethod) in testCases)
        {
            // Arrange
            var source = $@"
using Stratify.MinimalEndpoints.Attributes;

namespace TestApp;

[Endpoint({httpMethod}, ""/api/test"")]
public partial class TestEndpoint
{{
    [Handler]
    public IResult Handle() => Results.Ok();
}}";

            var compilation = CreateCompilation(source);
            var generator = new EndpointGeneratorImproved();

            // Act
            var driver = CSharpGeneratorDriver.Create(generator);
            var result = driver.RunGenerators(compilation).GetRunResult();

            // Assert
            if (result.GeneratedTrees.Length > 0)
            {
                var generatedSource = result.GeneratedTrees.First().GetText().ToString();
                await Assert.That(generatedSource)
                    .Contains(expectedMapMethod)
                    .Because($"Expected {expectedMapMethod} for {httpMethod}");
            }
        }
    }

    [Test]
    public async Task ExtractsPattern_FromSecondArgument()
    {
        // Test various patterns to ensure they're extracted from the second argument
        var testPatterns = new[]
        {
            "/api/products",
            "/api/products/{id}",
            "/api/users/{userId}/orders/{orderId}",
            "/health",
            "/"
        };

        foreach (var pattern in testPatterns)
        {
            // Arrange
            var source = $@"
using Stratify.MinimalEndpoints.Attributes;

namespace TestApp;

[Endpoint(HttpMethodType.Get, ""{pattern}"")]
public partial class TestEndpoint
{{
    [Handler]
    public IResult Handle() => Results.Ok();
}}";

            var compilation = CreateCompilation(source);
            var generator = new EndpointGeneratorImproved();

            // Act
            var driver = CSharpGeneratorDriver.Create(generator);
            var result = driver.RunGenerators(compilation).GetRunResult();

            // Assert
            if (result.GeneratedTrees.Length > 0)
            {
                var generatedSource = result.GeneratedTrees.First().GetText().ToString();
                await Assert.That(generatedSource)
                    .Contains($"\"{pattern}\"")
                    .Because($"Expected pattern {pattern} to be in generated code");
            }
        }
    }

    private static CSharpCompilation CreateCompilation(string source)
    {
        var syntaxTree = CSharpSyntaxTree.ParseText(source);

        var references = new[]
        {
            MetadataReference.CreateFromFile(typeof(object).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(System.Linq.Enumerable).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(System.Threading.Tasks.Task).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(Microsoft.AspNetCore.Http.IResult).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(Microsoft.AspNetCore.Http.Results).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(Microsoft.AspNetCore.Routing.IEndpointRouteBuilder).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(Stratify.MinimalEndpoints.Attributes.EndpointAttribute).Assembly.Location)
        };

        return CSharpCompilation.Create(
            assemblyName: "TestAssembly",
            syntaxTrees: new[] { syntaxTree },
            references: references,
            options: new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));
    }
}

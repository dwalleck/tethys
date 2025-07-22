using System.Collections.Immutable;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.Text;
using Stratify.MinimalEndpoints.ImprovedSourceGenerators;
using Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests.Helpers;
using TUnit.Assertions;
using TUnit.Assertions.Extensions;
using TUnit.Core;

namespace Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests;

public class EndpointGeneratorImprovedTests
{
    [Test]
    public async Task ImprovedGenerator_Should_Generate_IEndpoint_Implementation()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/test")]
            public partial class TestEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok("Test"));
                }
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunImprovedGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();

        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        await Assert.That(generatedFiles.Count).IsEqualTo(1);

        var generatedContent = generatedFiles[0].Content;
        await Assert.That(generatedContent).Contains("IEndpoint");
        await Assert.That(generatedContent).Contains("MapEndpoint");
        await Assert.That(generatedContent).Contains("MapGet");
        await Assert.That(generatedContent).Contains("HandleAsync");
    }

    [Test]
    public async Task ImprovedGenerator_Should_Use_ForAttributeWithMetadataName()
    {
        // This test verifies the generator only processes classes with [Endpoint] attribute
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            // This should be processed
            [Endpoint(HttpMethodType.Post, "/api/create")]
            public partial class CreateEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok());
                }
            }

            // This should NOT be processed (no [Endpoint] attribute)
            public partial class NotAnEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok());
                }
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunImprovedGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();

        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        await Assert.That(generatedFiles.Count).IsEqualTo(1);

        var generatedContent = generatedFiles[0].Content;
        await Assert.That(generatedContent).Contains("CreateEndpoint");
        await Assert.That(generatedContent).DoesNotContain("NotAnEndpoint");
    }

    private async Task<(Compilation outputCompilation, ImmutableArray<Diagnostic> diagnostics)> RunImprovedGenerator(string source)
    {
        var generator = new EndpointGeneratorImproved();
        var driver = CSharpGeneratorDriver.Create(generator);

        var compilation = TestCompilationHelper.CreateCompilation(source);
        driver.RunGeneratorsAndUpdateCompilation(compilation, out var outputCompilation, out var diagnostics);

        return (outputCompilation, diagnostics);
    }
}

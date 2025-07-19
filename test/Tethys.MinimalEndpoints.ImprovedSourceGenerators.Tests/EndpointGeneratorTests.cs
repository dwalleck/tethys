using System.Collections.Immutable;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.Text;
using Tethys.MinimalEndpoints.ImprovedSourceGenerators;
using Tethys.MinimalEndpoints.ImprovedSourceGenerators.Tests.Helpers;
using TUnit.Assertions;
using TUnit.Assertions.Extensions;
using TUnit.Core;

namespace Tethys.MinimalEndpoints.ImprovedSourceGenerators.Tests;

public class EndpointGeneratorTests
{
    [Test]
    public async Task Generator_Should_Generate_IEndpoint_Implementation_For_Attribute_Class()
    {
        // Arrange
        var source = """
            using System;
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethod.Get, "/api/products")]
            public partial class GetProductsEndpoint
            {
                [Handler]
                public async Task<IResult> HandleAsync()
                {
                    return Results.Ok("Products");
                }
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();
        
        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        await Assert.That(generatedFiles.Count).IsEqualTo(1);
        
        var generatedFile = generatedFiles[0];
        await Assert.That(generatedFile.FileName).Contains("GeneratedEndpoints.g.cs");
        await Assert.That(generatedFile.Content).Contains("partial class GetProductsEndpoint : IEndpoint");
        await Assert.That(generatedFile.Content).Contains("public void MapEndpoint(IEndpointRouteBuilder app)");
        await Assert.That(generatedFile.Content).Contains("app.MapGet(\"/api/products\", HandleAsync)");
    }

    [Test]
    public async Task Generator_Should_Skip_Classes_Without_Endpoint_Attribute()
    {
        // Arrange
        var source = """
            using Microsoft.AspNetCore.Http;

            namespace TestApp.Features.Products;

            public class NotAnEndpoint
            {
                public IResult Handle()
                {
                    return Results.Ok();
                }
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();
        
        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        await Assert.That(generatedFiles).IsEmpty();
    }

    [Test]
    public async Task Generator_Should_Skip_Non_Partial_Classes()
    {
        // Arrange
        var source = """
            using Tethys.MinimalEndpoints;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethod.Get, "/api/test")]
            public class NonPartialEndpoint
            {
                [Handler]
                public IResult Handle() => Results.Ok();
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();
        
        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        await Assert.That(generatedFiles).IsEmpty();
    }

    [Test]
    public async Task Generator_Should_Handle_Classes_Without_Handler_Method()
    {
        // Arrange
        var source = """
            using Tethys.MinimalEndpoints;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethod.Get, "/api/test")]
            public partial class EndpointWithoutHandler
            {
                // No [Handler] attribute
                public IResult Handle() => Results.Ok();
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();
        
        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        // The generator should either skip this or use a default handler name
        if (generatedFiles.Count > 0)
        {
            await Assert.That(generatedFiles[0].Content).Contains("HandleAsync");
        }
    }

    private async Task<(Compilation outputCompilation, ImmutableArray<Diagnostic> diagnostics)> RunGenerator(string source)
    {
        var generator = new EndpointGeneratorImproved();
        var driver = CSharpGeneratorDriver.Create(generator);
        
        var compilation = TestCompilationHelper.CreateCompilation(source);
        driver.RunGeneratorsAndUpdateCompilation(compilation, out var outputCompilation, out var diagnostics);
        
        return (outputCompilation, diagnostics);
    }
}
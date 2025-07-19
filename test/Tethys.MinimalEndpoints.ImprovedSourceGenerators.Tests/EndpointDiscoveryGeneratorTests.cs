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

public class EndpointDiscoveryGeneratorTests
{
    [Test]
    public async Task Generator_Should_Discover_Single_Endpoint()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethod.Post, "/products")]
            public partial class CreateProductEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok("Created"));
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
        await Assert.That(generatedFile.Content).Contains("partial class CreateProductEndpoint : IEndpoint");
        await Assert.That(generatedFile.Content).Contains("public void MapEndpoint(IEndpointRouteBuilder app)");
        await Assert.That(generatedFile.Content).Contains("app.MapPost(\"/products\", HandleAsync)");
    }

    [Test]
    public async Task Generator_Should_Discover_Multiple_Endpoints()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethod.Post, "/products")]
            public partial class CreateProductEndpoint
            {
                [Handler]
                public Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok("Created"));
                }
            }

            [Endpoint(HttpMethod.Get, "/products/{id}")]
            public partial class GetProductEndpoint
            {
                [Handler]
                public Task<IResult> HandleAsync(int id)
                {
                    return Task.FromResult(Results.Ok($"Product {id}"));
                }
            }

            namespace TestApp.Features.Orders;

            [Endpoint(HttpMethod.Post, "/orders")]
            public partial class CreateOrderEndpoint
            {
                [Handler]
                public Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok("Order created"));
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
        await Assert.That(generatedFile.Content).Contains("CreateProductEndpoint");
        await Assert.That(generatedFile.Content).Contains("GetProductEndpoint");
        await Assert.That(generatedFile.Content).Contains("CreateOrderEndpoint");
    }

    [Test]
    public async Task Generator_Should_Ignore_Abstract_Classes()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethod.Post, "/products")]
            public abstract partial class BaseEndpoint
            {
                [Handler]
                public abstract Task<IResult> HandleAsync();
            }

            [Endpoint(HttpMethod.Post, "/products")]
            public partial class CreateProductEndpoint
            {
                [Handler]
                public Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok("Created"));
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
        // Abstract class should not be generated
        await Assert.That(generatedFile.Content).DoesNotContain("BaseEndpoint");
        // Concrete class should be generated
        await Assert.That(generatedFile.Content).Contains("CreateProductEndpoint");
    }

    [Test]
    public async Task Generator_Should_Not_Generate_Extension_Methods()
    {
        // Arrange - Our generator doesn't create extension methods for discovery
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethod.Post, "/products")]
            public partial class CreateProductEndpoint
            {
                [Handler]
                public Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok("Created"));
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
        
        // Our generator creates partial class implementations, not extension methods
        var generatedFile = generatedFiles[0];
        await Assert.That(generatedFile.Content).DoesNotContain("public static class");
        await Assert.That(generatedFile.Content).Contains("partial class CreateProductEndpoint");
    }

    [Test]
    public async Task Generator_Should_Handle_Nested_Classes()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp.Features.Products;

            public static class CreateProduct
            {
                [Endpoint(HttpMethod.Post, "/products")]
                public partial class Endpoint
                {
                    [Handler]
                    public Task<IResult> HandleAsync()
                    {
                        return Task.FromResult(Handler());
                    }
                }

                private static IResult Handler()
                {
                    return Results.Ok("Created");
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
        await Assert.That(generatedFile.Content).Contains("Endpoint");
        await Assert.That(generatedFile.Content).Contains("MapPost");
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
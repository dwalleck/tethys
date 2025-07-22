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

public class HttpMethodEnumGeneratorTests
{
    [Test]
    public async Task Generator_Should_Correctly_Extract_HttpMethod_From_Enum_Attribute()
    {
        // Arrange - This tests the specific enum conversion issue
        var source = """
            using System;
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethod.Post, "/api/products")]
            public partial class CreateProductEndpoint
            {
                [Handler]
                public async Task<IResult> HandleAsync(CreateProductRequest request)
                {
                    return Results.Ok(new { Id = Guid.NewGuid() });
                }
            }

            public record CreateProductRequest(string Name, decimal Price);
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();

        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        await Assert.That(generatedFiles.Count).IsGreaterThanOrEqualTo(1);

        var generatedFile = generatedFiles.FirstOrDefault(f => f.Content.Contains("MapPost"));
        await Assert.That(generatedFile.FileName).IsNotNull();
        await Assert.That(generatedFile.Content).Contains("app.MapPost(\"/api/products\"");
    }

    [Test]
    public async Task Generator_Should_Handle_All_HttpMethod_Enum_Values()
    {
        // Arrange
        var httpMethods = new[] { "Get", "Post", "Put", "Delete", "Patch" };

        foreach (var method in httpMethods)
        {
            var source = $$"""
                using System;
                using System.Threading.Tasks;
                using Microsoft.AspNetCore.Http;
                using Stratify.MinimalEndpoints;

                namespace TestApp.Features.Test;

                [Endpoint(HttpMethod.{{method}}, "/api/test")]
                public partial class TestEndpoint
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
            var generatedFile = generatedFiles.FirstOrDefault();
            var generatedContent = generatedFile.Content ?? "";

            var expectedMapMethod = $"Map{method}";
            await Assert.That(generatedContent).Contains(expectedMapMethod);
        }
    }

    [Test]
    public async Task Generator_Should_Handle_Enum_Value_Extraction_Correctly()
    {
        // Arrange - Test numeric enum values
        var source = """
            using System;
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Put, "/api/items/{id}")]
            public partial class UpdateItemEndpoint
            {
                [Handler]
                public IResult Handle(int id) => Results.NoContent();
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();

        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        var generatedFile = generatedFiles.FirstOrDefault();
        var generatedContent = generatedFile.Content ?? "";

        await Assert.That(generatedContent).Contains("MapPut");
        await Assert.That(generatedContent).Contains("/api/items/{id}");
    }

    [Test]
    public async Task Generator_Should_Handle_Invalid_Enum_Values_Gracefully()
    {
        // Arrange - Test with an invalid enum value
        var source = """
            using System;
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints;

            namespace TestApp;

            // This should not generate anything because it's missing the handler
            [Endpoint(HttpMethod.Get, "/api/invalid")]
            public partial class InvalidEndpoint
            {
                // No [Handler] attribute
                public IResult Handle() => Results.Ok();
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunGenerator(source);

        // Assert - Generator should handle this gracefully
        var errors = diagnostics.Where(d => d.Severity == DiagnosticSeverity.Error);
        await Assert.That(errors).IsEmpty();
    }

    [Test]
    public async Task Generator_Should_Properly_Cast_Enum_Values()
    {
        // Arrange - Test the specific casting issue
        var source = """
            using System;
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Delete, "/api/users/{id}")]
            public partial class DeleteUserEndpoint
            {
                [Handler]
                public IResult Handle(int id) => Results.NoContent();
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();

        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        var generatedFile = generatedFiles.FirstOrDefault();
        var generatedContent = generatedFile.Content ?? "";

        await Assert.That(generatedContent).Contains("MapDelete");
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

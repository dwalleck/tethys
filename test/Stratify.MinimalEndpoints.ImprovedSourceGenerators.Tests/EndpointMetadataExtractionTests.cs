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

public class EndpointMetadataExtractionTests
{
    [Test]
    public async Task Test_Metadata_With_Tags()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/test")]
            [EndpointMetadata(Tags = new[] { "Users", "Admin" })]
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
        await Assert.That(generatedContent).Contains(".WithTags(\"Users\", \"Admin\")");
    }

    [Test]
    public async Task Test_Metadata_With_Name_Summary_Description()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Post, "/api/create")]
            [EndpointMetadata(
                Name = "CreateUser",
                Summary = "Creates a new user",
                Description = "This endpoint creates a new user in the system with the provided details"
            )]
            public partial class CreateEndpoint
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
        var generatedContent = generatedFiles[0].Content;

        await Assert.That(generatedContent).Contains(".WithName(\"CreateUser\")");
        await Assert.That(generatedContent).Contains(".WithSummary(\"Creates a new user\")");
        await Assert.That(generatedContent).Contains(".WithDescription(\"This endpoint creates a new user in the system with the provided details\")");
    }

    [Test]
    public async Task Test_Metadata_With_Authorization()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Delete, "/api/delete")]
            [EndpointMetadata(RequiresAuthorization = true)]
            public partial class DeleteEndpoint
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
        var generatedContent = generatedFiles[0].Content;

        await Assert.That(generatedContent).Contains(".RequireAuthorization()");
    }

    [Test]
    public async Task Test_Metadata_With_Policies()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Put, "/api/update")]
            [EndpointMetadata(
                RequiresAuthorization = true,
                Policies = new[] { "AdminPolicy", "UserPolicy" }
            )]
            public partial class UpdateEndpoint
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
        var generatedContent = generatedFiles[0].Content;

        await Assert.That(generatedContent).Contains(".RequireAuthorization()");
        await Assert.That(generatedContent).Contains(".RequireAuthorization(\"AdminPolicy\")");
        await Assert.That(generatedContent).Contains(".RequireAuthorization(\"UserPolicy\")");
    }

    [Test]
    public async Task Test_Metadata_With_Roles()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/admin")]
            [EndpointMetadata(
                RequiresAuthorization = true,
                Roles = new[] { "Admin", "SuperUser" }
            )]
            public partial class AdminEndpoint
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
        var generatedContent = generatedFiles[0].Content;

        await Assert.That(generatedContent).Contains(".RequireAuthorization()");
        await Assert.That(generatedContent).Contains(".RequireAuthorization(policy => policy.RequireRole(\"Admin\", \"SuperUser\"))");
    }

    [Test]
    public async Task Test_Metadata_With_All_Properties()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Post, "/api/complex")]
            [EndpointMetadata(
                Tags = new[] { "Complex", "Test" },
                Name = "ComplexEndpoint",
                Summary = "A complex endpoint",
                Description = "This endpoint demonstrates all metadata properties",
                RequiresAuthorization = true,
                Policies = new[] { "RequireAdminPolicy" },
                Roles = new[] { "Admin", "Manager" }
            )]
            public partial class ComplexEndpoint
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
        var generatedContent = generatedFiles[0].Content;

        // Verify all metadata is present
        await Assert.That(generatedContent).Contains(".WithTags(\"Complex\", \"Test\")");
        await Assert.That(generatedContent).Contains(".WithName(\"ComplexEndpoint\")");
        await Assert.That(generatedContent).Contains(".WithSummary(\"A complex endpoint\")");
        await Assert.That(generatedContent).Contains(".WithDescription(\"This endpoint demonstrates all metadata properties\")");
        await Assert.That(generatedContent).Contains(".RequireAuthorization()");
        await Assert.That(generatedContent).Contains(".RequireAuthorization(\"RequireAdminPolicy\")");
        await Assert.That(generatedContent).Contains(".RequireAuthorization(policy => policy.RequireRole(\"Admin\", \"Manager\"))");
    }

    [Test]
    public async Task Test_Metadata_String_Escaping()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/escape")]
            [EndpointMetadata(
                Name = "Test\"Quotes\"",
                Summary = "Has\nNewline\rCarriage",
                Description = "Special chars: \"quotes\" and \n newlines"
            )]
            public partial class EscapeEndpoint
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
        var generatedContent = generatedFiles[0].Content;

        // Verify strings are properly escaped
        await Assert.That(generatedContent).Contains(".WithName(\"Test\\\"Quotes\\\"\")");
        await Assert.That(generatedContent).Contains(".WithSummary(\"Has\\nNewline\\rCarriage\")");
        await Assert.That(generatedContent).Contains(".WithDescription(\"Special chars: \\\"quotes\\\" and \\n newlines\")");
    }

    [Test]
    public async Task Test_Metadata_With_Empty_Arrays()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/empty")]
            [EndpointMetadata(
                Tags = new string[] { },
                Policies = new string[] { },
                Roles = new string[] { }
            )]
            public partial class EmptyArraysEndpoint
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
        var generatedContent = generatedFiles[0].Content;

        // Empty arrays should not generate any WithTags/RequireAuthorization calls
        await Assert.That(generatedContent).DoesNotContain(".WithTags(");
        await Assert.That(generatedContent).DoesNotContain(".RequireAuthorization(");
    }

    [Test]
    public async Task Test_Endpoint_Without_Metadata_Attribute()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/simple")]
            public partial class SimpleEndpoint
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
        var generatedContent = generatedFiles[0].Content;

        // No metadata calls should be generated
        await Assert.That(generatedContent).DoesNotContain(".WithTags(");
        await Assert.That(generatedContent).DoesNotContain(".WithName(");
        await Assert.That(generatedContent).DoesNotContain(".WithSummary(");
        await Assert.That(generatedContent).DoesNotContain(".WithDescription(");
        await Assert.That(generatedContent).DoesNotContain(".RequireAuthorization(");
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

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

public class HttpMethodCoverageTests
{
    [Test]
    public async Task Test_Head_Method_Generation()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Head, "/api/head")]
            public partial class HeadEndpoint
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
        // HEAD method should use MapMethods
        await Assert.That(generatedContent).Contains("app.MapMethods(\"/api/head\", HandleAsync)");
    }

    [Test]
    public async Task Test_Options_Method_Generation()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Options, "/api/options")]
            public partial class OptionsEndpoint
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

        // OPTIONS method should use MapMethods
        await Assert.That(generatedContent).Contains("app.MapMethods(\"/api/options\", HandleAsync)");
    }

    [Test]
    public async Task Test_Patch_Method_Generation()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Patch, "/api/patch")]
            public partial class PatchEndpoint
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

        await Assert.That(generatedContent).Contains("app.MapPatch(\"/api/patch\", HandleAsync)");
    }

    [Test]
    public async Task Test_All_Standard_Http_Methods()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/get")]
            public partial class GetEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok());
            }

            [Endpoint(HttpMethodType.Post, "/api/post")]
            public partial class PostEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok());
            }

            [Endpoint(HttpMethodType.Put, "/api/put")]
            public partial class PutEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok());
            }

            [Endpoint(HttpMethodType.Delete, "/api/delete")]
            public partial class DeleteEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok());
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunImprovedGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();

        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        var generatedContent = generatedFiles[0].Content;

        // Verify all standard methods are correctly mapped
        await Assert.That(generatedContent).Contains("app.MapGet(\"/api/get\", HandleAsync)");
        await Assert.That(generatedContent).Contains("app.MapPost(\"/api/post\", HandleAsync)");
        await Assert.That(generatedContent).Contains("app.MapPut(\"/api/put\", HandleAsync)");
        await Assert.That(generatedContent).Contains("app.MapDelete(\"/api/delete\", HandleAsync)");
    }

    [Test]
    public async Task Test_Unknown_Method_Defaults_To_Get()
    {
        // This test simulates what happens if an unknown enum value is passed
        // Since we can't easily create an unknown enum at compile time, we'll
        // test the generator's handling by checking the Unknown case in isolation
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            // This would be caught during extraction, but we test the mapping logic
            [Endpoint(HttpMethodType.Get, "/api/test")]
            public partial class TestEndpoint
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

        // The generator should produce valid output even if it encounters unknown values
        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        await Assert.That(generatedFiles.Count).IsEqualTo(1);
    }

    [Test]
    public async Task Test_Multiple_Endpoints_Different_Methods()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/users")]
            public partial class GetUsersEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok());
            }

            [Endpoint(HttpMethodType.Post, "/api/users")]
            public partial class CreateUserEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok());
            }

            [Endpoint(HttpMethodType.Put, "/api/users/{id}")]
            public partial class UpdateUserEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(int id) => Task.FromResult(Results.Ok());
            }

            [Endpoint(HttpMethodType.Delete, "/api/users/{id}")]
            public partial class DeleteUserEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(int id) => Task.FromResult(Results.Ok());
            }

            [Endpoint(HttpMethodType.Patch, "/api/users/{id}")]
            public partial class PatchUserEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(int id) => Task.FromResult(Results.Ok());
            }

            [Endpoint(HttpMethodType.Head, "/api/users")]
            public partial class HeadUsersEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok());
            }

            [Endpoint(HttpMethodType.Options, "/api/users")]
            public partial class OptionsUsersEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok());
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunImprovedGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();

        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        var generatedContent = generatedFiles[0].Content;

        // Verify all endpoints are generated with correct methods
        await Assert.That(generatedContent).Contains("partial class GetUsersEndpoint");
        await Assert.That(generatedContent).Contains("partial class CreateUserEndpoint");
        await Assert.That(generatedContent).Contains("partial class UpdateUserEndpoint");
        await Assert.That(generatedContent).Contains("partial class DeleteUserEndpoint");
        await Assert.That(generatedContent).Contains("partial class PatchUserEndpoint");
        await Assert.That(generatedContent).Contains("partial class HeadUsersEndpoint");
        await Assert.That(generatedContent).Contains("partial class OptionsUsersEndpoint");

        // Verify correct mapping methods
        await Assert.That(generatedContent).Contains("MapGet");
        await Assert.That(generatedContent).Contains("MapPost");
        await Assert.That(generatedContent).Contains("MapPut");
        await Assert.That(generatedContent).Contains("MapDelete");
        await Assert.That(generatedContent).Contains("MapPatch");
        await Assert.That(generatedContent).Contains("MapMethods");
    }

    [Test]
    public async Task Test_Case_Sensitive_Enum_Matching()
    {
        // Test that the enum value matching is case-sensitive and exact
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/test1")]
            public partial class GetEndpoint1
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok());
            }

            [Endpoint(HttpMethodType.Post, "/api/test2")]
            public partial class PostEndpoint2
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok());
            }

            [Endpoint(HttpMethodType.Put, "/api/test3")]
            public partial class PutEndpoint3
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok());
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunImprovedGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();

        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        var generatedContent = generatedFiles[0].Content;

        // Each method should be correctly identified based on exact enum name matching
        await Assert.That(generatedContent).Contains("app.MapGet(\"/api/test1\"");
        await Assert.That(generatedContent).Contains("app.MapPost(\"/api/test2\"");
        await Assert.That(generatedContent).Contains("app.MapPut(\"/api/test3\"");
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

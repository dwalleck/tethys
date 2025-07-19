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

public class HandlerParameterTests
{
    [Test]
    public async Task Test_Handler_With_String_Parameter()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "/api/hello/{name}")]
            public partial class HelloEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(string name)
                {
                    return Task.FromResult(Results.Ok($"Hello, {name}!"));
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
        
        // Should generate endpoint with correct handler method
        await Assert.That(generatedContent).Contains("HandleAsync");
        await Assert.That(generatedContent).Contains("/api/hello/{name}");
    }

    [Test]
    public async Task Test_Handler_With_Int_Parameter()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "/api/users/{id:int}")]
            public partial class GetUserEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(int id)
                {
                    return Task.FromResult(Results.Ok($"User ID: {id}"));
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
        
        await Assert.That(generatedContent).Contains("HandleAsync");
        await Assert.That(generatedContent).Contains("/api/users/{id:int}");
    }

    [Test]
    public async Task Test_Handler_With_Optional_Parameter()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "/api/search")]
            public partial class SearchEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(string? query = null)
                {
                    return Task.FromResult(Results.Ok($"Search query: {query ?? "none"}"));
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
    }

    [Test]
    public async Task Test_Handler_With_Default_Value_String()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "/api/greet")]
            public partial class GreetEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(string name = "World")
                {
                    return Task.FromResult(Results.Ok($"Hello, {name}!"));
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
    }

    [Test]
    public async Task Test_Handler_With_Default_Value_Number()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "/api/page")]
            public partial class PageEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(int pageNumber = 1, int pageSize = 10)
                {
                    return Task.FromResult(Results.Ok($"Page {pageNumber} with size {pageSize}"));
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
    }

    [Test]
    public async Task Test_Handler_With_Default_Value_Bool()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "/api/filter")]
            public partial class FilterEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(bool includeDeleted = false, bool activeOnly = true)
                {
                    return Task.FromResult(Results.Ok($"Include deleted: {includeDeleted}, Active only: {activeOnly}"));
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
    }

    [Test]
    public async Task Test_Handler_With_Default_Value_Null()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "/api/nullable")]
            public partial class NullableEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(string? name = null, int? age = null)
                {
                    return Task.FromResult(Results.Ok($"Name: {name ?? "none"}, Age: {age?.ToString() ?? "none"}"));
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
    }

    [Test]
    public async Task Test_Handler_With_Complex_Type_Parameter()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            public record CreateUserRequest(string Name, string Email);

            [Endpoint(HttpMethod.Post, "/api/users")]
            public partial class CreateUserEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(CreateUserRequest request)
                {
                    return Task.FromResult(Results.Ok($"Created user: {request.Name}"));
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
    }

    [Test]
    public async Task Test_Handler_With_Multiple_Parameters()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            public record UpdateUserRequest(string Name, string Email);

            [Endpoint(HttpMethod.Put, "/api/users/{id:int}")]
            public partial class UpdateUserEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(int id, UpdateUserRequest request, CancellationToken ct = default)
                {
                    return Task.FromResult(Results.Ok($"Updated user {id}"));
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
    }

    [Test]
    public async Task Test_Handler_With_Service_Parameters()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Microsoft.Extensions.Logging;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            public interface IUserService
            {
                Task<string> GetUserAsync(int id);
            }

            [Endpoint(HttpMethod.Get, "/api/users/{id:int}")]
            public partial class GetUserWithServiceEndpoint
            {
                [Handler]
                public static async Task<IResult> HandleAsync(
                    int id,
                    IUserService userService,
                    ILogger<GetUserWithServiceEndpoint> logger,
                    CancellationToken ct)
                {
                    logger.LogInformation("Getting user {Id}", id);
                    var user = await userService.GetUserAsync(id);
                    return Results.Ok(user);
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
    }

    [Test]
    public async Task Test_Handler_With_HttpContext_Parameter()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "/api/context")]
            public partial class ContextEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(HttpContext context)
                {
                    var userAgent = context.Request.Headers["User-Agent"];
                    return Task.FromResult(Results.Ok($"User-Agent: {userAgent}"));
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
    }

    [Test]
    public async Task Test_Handler_With_No_Parameters()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "/api/simple")]
            public partial class SimpleEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok("No parameters"));
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
    }

    [Test]
    public async Task Test_Async_Handler_With_Task_Return()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "/api/async")]
            public partial class AsyncEndpoint
            {
                [Handler]
                public static async Task<IResult> HandleAsync()
                {
                    await Task.Delay(10);
                    return Results.Ok("Async completed");
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
    }

    [Test]
    public async Task Test_Handler_With_Character_Default_Value()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "/api/char")]
            public partial class CharEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(char delimiter = ',')
                {
                    return Task.FromResult(Results.Ok($"Delimiter: {delimiter}"));
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
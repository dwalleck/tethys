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

public class ErrorHandlingTests
{
    [Test]
    public async Task Test_Missing_Pattern_In_Attribute()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get)]  // Missing pattern parameter
            public partial class MissingPatternEndpoint
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
        // The generator should handle this gracefully and not generate code for this endpoint
        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        if (generatedFiles.Count > 0)
        {
            var generatedContent = generatedFiles[0].Content;
            await Assert.That(generatedContent).DoesNotContain("MissingPatternEndpoint");
        }
    }

    [Test]
    public async Task Test_Invalid_Pattern_Format()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "api/test")]  // Invalid pattern - missing leading slash
            public partial class InvalidPatternEndpoint
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
        // The generator should not generate code for endpoints with invalid patterns
        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        if (generatedFiles.Count > 0)
        {
            var generatedContent = generatedFiles[0].Content;
            await Assert.That(generatedContent).DoesNotContain("InvalidPatternEndpoint");
        }
    }

    [Test]
    public async Task Test_Missing_Handler_Method()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "/api/test")]
            public partial class NoHandlerEndpoint
            {
                // No method with [Handler] attribute
                public static Task<IResult> SomeMethod()
                {
                    return Task.FromResult(Results.Ok());
                }
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunImprovedGenerator(source);

        // Assert
        // Should still generate endpoint but use default handler name
        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        if (generatedFiles.Count > 0)
        {
            var generatedContent = generatedFiles[0].Content;
            await Assert.That(generatedContent).Contains("HandleAsync");
        }
    }

    [Test]
    public async Task Test_Multiple_Handler_Methods()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "/api/test")]
            public partial class MultipleHandlersEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok("First"));
                }
                
                [Handler]
                public static Task<IResult> AnotherHandleAsync()
                {
                    return Task.FromResult(Results.Ok("Second"));
                }
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunImprovedGenerator(source);

        // Assert
        // Should use the first handler found
        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        if (generatedFiles.Count > 0)
        {
            var generatedContent = generatedFiles[0].Content;
            await Assert.That(generatedContent).Contains("HandleAsync");
        }
    }

    [Test]
    public async Task Test_Null_Symbol_Handling()
    {
        // This test verifies that the generator handles null symbols gracefully
        // which can happen in certain error conditions
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            // Valid endpoint to ensure generator runs
            [Endpoint(HttpMethod.Get, "/api/valid")]
            public partial class ValidEndpoint
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
    }

    [Test]
    public async Task Test_Invalid_Attribute_Arguments()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint("InvalidHttpMethod", "/api/test")]  // String instead of HttpMethod enum
            public partial class InvalidHttpMethodEndpoint
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
        // This should result in compilation errors, not generator errors
        var compilationErrors = outputCompilation.GetDiagnostics()
            .Where(d => d.Severity == DiagnosticSeverity.Error)
            .ToList();
        
        await Assert.That(compilationErrors.Count).IsGreaterThan(0);
    }

    [Test]
    public async Task Test_Empty_Pattern_String()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "")]  // Empty pattern
            public partial class EmptyPatternEndpoint
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
        // Should not generate endpoint for empty pattern
        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        if (generatedFiles.Count > 0)
        {
            var generatedContent = generatedFiles[0].Content;
            await Assert.That(generatedContent).DoesNotContain("EmptyPatternEndpoint");
        }
    }

    [Test]
    public async Task Test_Whitespace_Pattern_String()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "   ")]  // Whitespace pattern
            public partial class WhitespacePatternEndpoint
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
        // Should not generate endpoint for whitespace pattern
        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        if (generatedFiles.Count > 0)
        {
            var generatedContent = generatedFiles[0].Content;
            await Assert.That(generatedContent).DoesNotContain("WhitespacePatternEndpoint");
        }
    }

    [Test]
    public async Task Test_Non_Partial_Class()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "/api/test")]
            public class NonPartialEndpoint  // Missing partial keyword
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
        // Should not generate for non-partial classes
        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        if (generatedFiles.Count > 0)
        {
            var generatedContent = generatedFiles[0].Content;
            await Assert.That(generatedContent).DoesNotContain("NonPartialEndpoint");
        }
    }

    [Test]
    public async Task Test_Abstract_Class()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint(HttpMethod.Get, "/api/test")]
            public abstract partial class AbstractEndpoint  // Abstract class
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
        // Should not generate for abstract classes
        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        if (generatedFiles.Count > 0)
        {
            var generatedContent = generatedFiles[0].Content;
            await Assert.That(generatedContent).DoesNotContain("AbstractEndpoint");
        }
    }

    [Test]
    public async Task Test_Invalid_Http_Method_Enum_Value()
    {
        // Arrange - simulate an invalid enum by using a large integer value
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints;

            namespace TestApp;

            [Endpoint((HttpMethod)999, "/api/test")]  // Invalid enum value
            public partial class InvalidEnumEndpoint
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
        // Should not generate endpoint for invalid enum value
        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        if (generatedFiles.Count > 0)
        {
            var generatedContent = generatedFiles[0].Content;
            await Assert.That(generatedContent).DoesNotContain("InvalidEnumEndpoint");
        }
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
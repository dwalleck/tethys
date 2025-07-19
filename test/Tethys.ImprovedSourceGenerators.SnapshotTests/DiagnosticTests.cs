using Microsoft.CodeAnalysis;
using TUnit.Assertions;
using TUnit.Core;

namespace Tethys.ImprovedSourceGenerators.SnapshotTests;

public class DiagnosticTests
{
    [Test]
    public Task ReportsDiagnostic_WhenHandlerMethodIsMissing()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/test")]
            public partial class TestEndpoint
            {
                // Missing [Handler] method
            }
            """;

        return TestHelper.Verify(source);
    }

    [Test]
    public Task ReportsDiagnostic_WhenInvalidHttpMethod()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint((HttpMethodType)999, "/api/test")] // Invalid HTTP method
            public partial class TestEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok());
                }
            }
            """;

        return TestHelper.Verify(source);
    }

    [Test]
    public Task ReportsDiagnostic_WhenInvalidPattern()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "api/test")] // Missing leading slash
            public partial class TestEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok());
                }
            }
            """;

        return TestHelper.Verify(source);
    }

    [Test]
    public Task ReportsDiagnostic_WhenMultipleHandlerMethods()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/test")]
            public partial class TestEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok());
                }

                [Handler]
                public static Task<IResult> AnotherHandleAsync()
                {
                    return Task.FromResult(Results.Ok());
                }
            }
            """;

        return TestHelper.Verify(source);
    }

    [Test]
    public async Task DoesNotReportDiagnostic_ForValidEndpoint()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp;

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

        var result = TestHelper.GetGeneratorRunResult(source);
        
        // Should have no error diagnostics
        var errors = result.Diagnostics.Where(d => d.Severity == DiagnosticSeverity.Error);
        await Assert.That(errors).IsEmpty();
        
        // Should generate output
        var generatedSources = result.Results[0].GeneratedSources;
        await Assert.That(generatedSources.Length).IsGreaterThan(0);
    }

    [Test]
    public Task ReportsDiagnostic_WhenEndpointAttributeHasWrongConstructorArguments()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint()] // Missing required arguments
            public partial class TestEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok());
                }
            }
            """;

        return TestHelper.Verify(source);
    }
}
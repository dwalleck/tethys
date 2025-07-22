using TUnit.Core;

namespace Stratify.ImprovedSourceGenerators.SnapshotTests;

public class AdvancedEndpointTests
{
    [Test]
    public Task GeneratesEndpoint_WithComplexHandlerSignatures()
    {
        var source = """
            using Stratify.MinimalEndpoints.Attributes;
            using Microsoft.AspNetCore.Http;
            using System.Threading;
            using System.Threading.Tasks;
            using Microsoft.Extensions.Logging;

            namespace TestApp.Features.Complex;

            [Endpoint(HttpMethodType.Put, "/api/complex/{id}")]
            public partial class ComplexHandlerEndpoint
            {
                [Handler]
                public static async Task<IResult> HandleAsync(
                    int id,
                    ComplexRequest request,
                    HttpContext httpContext,
                    ILogger<ComplexHandlerEndpoint> logger,
                    IService service,
                    CancellationToken cancellationToken = default,
                    string optionalParam = "default")
                {
                    logger.LogInformation("Processing request for {Id}", id);
                    await service.ProcessAsync(request, cancellationToken);
                    return Results.Ok();
                }
            }

            public record ComplexRequest(string Name, Dictionary<string, object> Properties);

            public interface IService
            {
                Task ProcessAsync(ComplexRequest request, CancellationToken cancellationToken);
            }
            """;

        return TestHelper.Verify(source);
    }

    [Test]
    public Task GeneratesEndpoint_WithEmptyMetadataArrays()
    {
        var source = """
            using Stratify.MinimalEndpoints.Attributes;
            using Microsoft.AspNetCore.Http;
            using System.Threading.Tasks;

            namespace TestApp.Features.Empty;

            [Endpoint(HttpMethodType.Get, "/api/empty")]
            [EndpointMetadata(
                Tags = new string[] { },
                Policies = new string[] { },
                Roles = new string[] { })]
            public partial class EmptyArraysEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok("Empty arrays"));
                }
            }
            """;

        return TestHelper.Verify(source);
    }

    [Test]
    public Task GeneratesEndpoints_ForAllHttpMethods()
    {
        var source = """
            using Stratify.MinimalEndpoints.Attributes;
            using Microsoft.AspNetCore.Http;
            using System.Threading.Tasks;

            namespace TestApp.Features.HttpMethods;

            [Endpoint(HttpMethodType.Get, "/api/method/get")]
            public partial class GetMethodEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok("GET"));
            }

            [Endpoint(HttpMethodType.Post, "/api/method/post")]
            public partial class PostMethodEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok("POST"));
            }

            [Endpoint(HttpMethodType.Put, "/api/method/put")]
            public partial class PutMethodEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok("PUT"));
            }

            [Endpoint(HttpMethodType.Delete, "/api/method/delete")]
            public partial class DeleteMethodEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok("DELETE"));
            }

            [Endpoint(HttpMethodType.Patch, "/api/method/patch")]
            public partial class PatchMethodEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok("PATCH"));
            }

            [Endpoint(HttpMethodType.Head, "/api/method/head")]
            public partial class HeadMethodEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok("HEAD"));
            }

            [Endpoint(HttpMethodType.Options, "/api/method/options")]
            public partial class OptionsMethodEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok("OPTIONS"));
            }
            """;

        return TestHelper.Verify(source);
    }

    [Test]
    public Task GeneratesEndpoints_ForNestedClasses()
    {
        var source = """
            using Stratify.MinimalEndpoints.Attributes;
            using Microsoft.AspNetCore.Http;
            using System.Threading.Tasks;

            namespace TestApp.Features.Nested;

            public class ParentClass
            {
                [Endpoint(HttpMethodType.Get, "/api/nested")]
                public partial class NestedEndpoint
                {
                    [Handler]
                    public static Task<IResult> HandleAsync()
                    {
                        return Task.FromResult(Results.Ok("Nested endpoint"));
                    }
                }

                public class AnotherLevel
                {
                    [Endpoint(HttpMethodType.Post, "/api/deeply-nested")]
                    public partial class DeeplyNestedEndpoint
                    {
                        [Handler]
                        public static Task<IResult> HandleAsync(string data)
                        {
                            return Task.FromResult(Results.Ok(data));
                        }
                    }
                }
            }
            """;

        return TestHelper.Verify(source);
    }
}
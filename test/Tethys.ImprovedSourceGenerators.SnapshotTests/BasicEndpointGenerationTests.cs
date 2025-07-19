using TUnit.Core;

namespace Tethys.ImprovedSourceGenerators.SnapshotTests;

public class BasicEndpointGenerationTests
{
    [Test]
    public Task GeneratesEndpointImplementation_ForSimpleGetEndpoint()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethodType.Get, "/api/products")]
            public partial class GetProductsEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok("Products"));
                }
            }
            """;

        return TestHelper.Verify(source);
    }

    [Test]
    public Task GeneratesEndpointImplementation_ForPostEndpointWithParameters()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp.Features.Products;

            public record CreateProductRequest(string Name, decimal Price);

            [Endpoint(HttpMethodType.Post, "/api/products")]
            public partial class CreateProductEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(CreateProductRequest request)
                {
                    return Task.FromResult(Results.Created($"/api/products/1", request));
                }
            }
            """;

        return TestHelper.Verify(source);
    }

    [Test]
    public Task GeneratesEndpointImplementation_ForPutEndpointWithRouteParameter()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethodType.Put, "/api/products/{id}")]
            public partial class UpdateProductEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(int id, UpdateProductRequest request)
                {
                    return Task.FromResult(Results.NoContent());
                }
            }

            public record UpdateProductRequest(string Name, decimal Price);
            """;

        return TestHelper.Verify(source);
    }

    [Test]
    public Task GeneratesEndpointImplementation_ForDeleteEndpoint()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethodType.Delete, "/api/products/{id}")]
            public partial class DeleteProductEndpoint
            {
                [Handler]
                public static async Task<IResult> HandleAsync(int id)
                {
                    await Task.Delay(100); // Simulate async work
                    return Results.NoContent();
                }
            }
            """;

        return TestHelper.Verify(source);
    }

    [Test]
    public Task DoesNotGenerateEndpoint_ForClassWithoutPartialModifier()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethodType.Get, "/api/test")]
            public class NotPartialEndpoint // Missing partial modifier
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
    public Task DoesNotGenerateEndpoint_ForAbstractClass()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethodType.Get, "/api/test")]
            public abstract partial class AbstractEndpoint
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
    public Task GeneratesMultipleEndpoints_InSameCompilation()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethodType.Get, "/api/products")]
            public partial class GetProductsEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync() => Task.FromResult(Results.Ok());
            }

            [Endpoint(HttpMethodType.Post, "/api/products")]
            public partial class CreateProductEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(CreateProductRequest request) 
                    => Task.FromResult(Results.Created("/api/products/1", request));
            }

            public record CreateProductRequest(string Name);
            """;

        return TestHelper.Verify(source);
    }
}
using TUnit.Core;

namespace Tethys.ImprovedSourceGenerators.SnapshotTests;

public class EndpointMetadataTests
{
    [Test]
    public Task GeneratesEndpoint_WithBasicMetadata()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethodType.Get, "/api/products")]
            [EndpointMetadata(
                Name = "GetProducts",
                Summary = "Gets all products",
                Description = "Returns a list of all products in the system",
                Tags = new[] { "Products", "Catalog" }
            )]
            public partial class GetProductsEndpoint
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
    public Task GeneratesEndpoint_WithAuthorizationMetadata()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp.Features.Admin;

            [Endpoint(HttpMethodType.Post, "/api/admin/users")]
            [EndpointMetadata(
                RequiresAuthorization = true,
                Policies = new[] { "AdminPolicy", "UserManagementPolicy" },
                Roles = new[] { "Admin", "SuperUser" }
            )]
            public partial class CreateUserEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(CreateUserRequest request)
                {
                    return Task.FromResult(Results.Created($"/api/users/1", request));
                }
            }

            public record CreateUserRequest(string Email, string Name);
            """;

        return TestHelper.Verify(source);
    }

    [Test]
    public Task GeneratesEndpoint_WithStringEscapingInMetadata()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp.Features.Test;

            [Endpoint(HttpMethodType.Get, "/api/test")]
            [EndpointMetadata(
                Name = "Test\"Endpoint",
                Summary = "This is a test with \"quotes\" and \nnewlines",
                Description = "Description with special chars: \r\n\t\"'\\",
                Tags = new[] { "Tag\"1", "Tag\n2" }
            )]
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
    public Task GeneratesEndpoint_WithPartialMetadata()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethodType.Get, "/api/products/{id}")]
            [EndpointMetadata(
                Name = "GetProductById",
                RequiresAuthorization = true
                // Other properties are omitted/null
            )]
            public partial class GetProductByIdEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(int id)
                {
                    return Task.FromResult(Results.Ok($"Product {id}"));
                }
            }
            """;

        return TestHelper.Verify(source);
    }

    [Test]
    public Task GeneratesEndpoint_WithoutMetadataAttribute()
    {
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Tethys.MinimalEndpoints.Attributes;

            namespace TestApp.Features.Products;

            [Endpoint(HttpMethodType.Get, "/api/products")]
            // No [EndpointMetadata] attribute
            public partial class GetProductsEndpoint
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
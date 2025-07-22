using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using TUnit.Core;
using VerifyTests;
using VerifyTUnit;
using Stratify.MinimalEndpoints.ImprovedSourceGenerators;
using System.Runtime.CompilerServices;
using System.IO;

namespace Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests;

public class EndpointGeneratorSnapshotTests
{
    [Test]
    public Task Snapshot_Basic_Endpoint()
    {
        var source = """
            using Stratify.MinimalEndpoints;

            namespace TestNamespace;

            [Endpoint(HttpMethod.Get, "/api/test")]
            public partial class TestEndpoint
            {
                public string Handle()
                {
                    return "Hello World";
                }
            }
            """;

        return VerifyGeneratedSource(source);
    }

    [Test]
    public Task Snapshot_Endpoint_With_Metadata()
    {
        var source = """
            using Stratify.MinimalEndpoints;

            namespace TestNamespace;

            [Endpoint(HttpMethod.Post, "/api/users")]
            [EndpointMetadata(
                Name = "CreateUser",
                Summary = "Creates a new user",
                Description = "Creates a new user in the system with the provided details",
                Tags = new[] { "Users", "Admin" },
                RequiresAuthorization = true,
                Policies = new[] { "AdminPolicy" },
                Roles = new[] { "Administrator", "UserManager" })]
            public partial class CreateUserEndpoint
            {
                public Task<IResult> Handle(UserDto user)
                {
                    return Task.FromResult(Results.Ok(user));
                }
            }

            public record UserDto(string Name, string Email);
            """;

        return VerifyGeneratedSource(source);
    }

    [Test]
    public Task Snapshot_Multiple_Endpoints()
    {
        var source = """
            using Stratify.MinimalEndpoints;
            using System.Threading.Tasks;

            namespace TestNamespace;

            [Endpoint(HttpMethod.Get, "/api/products")]
            public partial class GetProductsEndpoint
            {
                public Task<List<Product>> Handle()
                {
                    return Task.FromResult(new List<Product>());
                }
            }

            [Endpoint(HttpMethod.Get, "/api/products/{id}")]
            [EndpointMetadata(Name = "GetProductById")]
            public partial class GetProductByIdEndpoint
            {
                public Task<Product?> Handle(int id)
                {
                    return Task.FromResult<Product?>(null);
                }
            }

            [Endpoint(HttpMethod.Post, "/api/products")]
            [EndpointMetadata(RequiresAuthorization = true)]
            public partial class CreateProductEndpoint
            {
                public Task<IResult> Handle(Product product)
                {
                    return Task.FromResult(Results.Created($"/api/products/{product.Id}", product));
                }
            }

            public record Product(int Id, string Name, decimal Price);
            """;

        return VerifyGeneratedSource(source);
    }

    [Test]
    public Task Snapshot_Complex_Handler_Signatures()
    {
        var source = """
            using Stratify.MinimalEndpoints;
            using Microsoft.AspNetCore.Http;
            using System.Threading;
            using System.Threading.Tasks;

            namespace TestNamespace;

            [Endpoint(HttpMethod.Put, "/api/complex/{id}")]
            public partial class ComplexHandlerEndpoint
            {
                public async Task<IResult> Handle(
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

        return VerifyGeneratedSource(source);
    }

    [Test]
    public Task Snapshot_Endpoint_With_Special_Characters()
    {
        var source = """
            using Stratify.MinimalEndpoints;

            namespace TestNamespace;

            [Endpoint(HttpMethod.Get, "/api/special\"chars\"")]
            [EndpointMetadata(
                Name = "Special\"Name\"",
                Summary = "Summary with \"quotes\" and \nnewlines",
                Description = "Description with \"quotes\", \ttabs, and \\backslashes")]
            public partial class SpecialCharactersEndpoint
            {
                public string Handle()
                {
                    return "Special response";
                }
            }
            """;

        return VerifyGeneratedSource(source);
    }

    [Test]
    public Task Snapshot_Endpoint_With_Empty_Arrays()
    {
        var source = """
            using Stratify.MinimalEndpoints;

            namespace TestNamespace;

            [Endpoint(HttpMethod.Get, "/api/empty")]
            [EndpointMetadata(
                Tags = new string[] { },
                Policies = new string[] { },
                Roles = new string[] { })]
            public partial class EmptyArraysEndpoint
            {
                public string Handle()
                {
                    return "Empty arrays";
                }
            }
            """;

        return VerifyGeneratedSource(source);
    }

    [Test]
    public Task Snapshot_All_Http_Methods()
    {
        var source = """
            using Stratify.MinimalEndpoints;

            namespace TestNamespace;

            [Endpoint(HttpMethod.Get, "/api/method/get")]
            public partial class GetMethodEndpoint
            {
                public string Handle() => "GET";
            }

            [Endpoint(HttpMethod.Post, "/api/method/post")]
            public partial class PostMethodEndpoint
            {
                public string Handle() => "POST";
            }

            [Endpoint(HttpMethod.Put, "/api/method/put")]
            public partial class PutMethodEndpoint
            {
                public string Handle() => "PUT";
            }

            [Endpoint(HttpMethod.Delete, "/api/method/delete")]
            public partial class DeleteMethodEndpoint
            {
                public string Handle() => "DELETE";
            }

            [Endpoint(HttpMethod.Patch, "/api/method/patch")]
            public partial class PatchMethodEndpoint
            {
                public string Handle() => "PATCH";
            }

            [Endpoint(HttpMethod.Head, "/api/method/head")]
            public partial class HeadMethodEndpoint
            {
                public string Handle() => "HEAD";
            }

            [Endpoint(HttpMethod.Options, "/api/method/options")]
            public partial class OptionsMethodEndpoint
            {
                public string Handle() => "OPTIONS";
            }
            """;

        return VerifyGeneratedSource(source);
    }

    [Test]
    public Task Snapshot_Nested_Classes()
    {
        var source = """
            using Stratify.MinimalEndpoints;

            namespace TestNamespace;

            public class ParentClass
            {
                [Endpoint(HttpMethod.Get, "/api/nested")]
                public partial class NestedEndpoint
                {
                    public string Handle()
                    {
                        return "Nested endpoint";
                    }
                }

                public class AnotherLevel
                {
                    [Endpoint(HttpMethod.Post, "/api/deeply-nested")]
                    public partial class DeeplyNestedEndpoint
                    {
                        public IResult Handle(string data)
                        {
                            return Results.Ok(data);
                        }
                    }
                }
            }
            """;

        return VerifyGeneratedSource(source);
    }

    private static Task VerifyGeneratedSource(string source)
    {
        // Add the EndpointAttribute and HttpMethod definitions
        var attributeSource = """
            namespace Stratify.MinimalEndpoints.Attributes
            {
                [System.AttributeUsage(System.AttributeTargets.Class)]
                public class EndpointAttribute : System.Attribute
                {
                    public string Pattern { get; }
                    public HttpMethodType Method { get; }

                    public EndpointAttribute(HttpMethodType method, string pattern)
                    {
                        Method = method;
                        Pattern = pattern;
                    }
                }

                [System.AttributeUsage(System.AttributeTargets.Class)]
                public class EndpointMetadataAttribute : System.Attribute
                {
                    public string? Name { get; set; }
                    public string? Summary { get; set; }
                    public string? Description { get; set; }
                    public string[]? Tags { get; set; }
                    public bool RequiresAuthorization { get; set; }
                    public string[]? Policies { get; set; }
                    public string[]? Roles { get; set; }
                }

                public enum HttpMethodType
                {
                    Get,
                    Post,
                    Put,
                    Delete,
                    Patch,
                    Head,
                    Options
                }
            }

            namespace Stratify.MinimalEndpoints
            {
                // Re-export for convenience
                using Stratify.MinimalEndpoints.Attributes;
                using HttpMethod = Stratify.MinimalEndpoints.Attributes.HttpMethodType;
            }

            namespace System.Threading.Tasks
            {
                public class Task
                {
                    public static Task<T> FromResult<T>(T result) => null!;
                }

                public class Task<T>
                {
                }
            }

            namespace Microsoft.AspNetCore.Http
            {
                public interface IResult { }
                public static class Results
                {
                    public static IResult Ok() => null!;
                    public static IResult Ok(object value) => null!;
                    public static IResult Created(string uri, object value) => null!;
                }
                public class HttpContext { }
            }

            namespace Microsoft.Extensions.Logging
            {
                public interface ILogger<T>
                {
                    void LogInformation(string message, params object[] args);
                }
            }

            namespace System.Collections.Generic
            {
                public class List<T> { }
                public class Dictionary<TKey, TValue> { }
            }
            """;

        var syntaxTree = CSharpSyntaxTree.ParseText(source);
        var attributeSyntaxTree = CSharpSyntaxTree.ParseText(attributeSource);

        var references = new[]
        {
            MetadataReference.CreateFromFile(typeof(object).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(System.Linq.Enumerable).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(System.Threading.CancellationToken).Assembly.Location)
        };

        var compilation = CSharpCompilation.Create(
            assemblyName: "TestAssembly",
            syntaxTrees: new[] { attributeSyntaxTree, syntaxTree },
            references: references,
            options: new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));

        var generator = new EndpointGeneratorImproved();
        var driver = CSharpGeneratorDriver.Create(generator);

        driver = (CSharpGeneratorDriver)driver.RunGeneratorsAndUpdateCompilation(compilation, out var outputCompilation, out var diagnostics);

        var runResult = driver.GetRunResult();
        var generatorResult = runResult.Results[0];

        return Verify(generatorResult.GeneratedSources.Select(gs => new
        {
            gs.HintName,
            Source = gs.SourceText.ToString()
        }));
    }
}

public static class ModuleInit
{
    [ModuleInitializer]
    public static void Init()
    {
        // Configure Verify to use a subdirectory for snapshots
        Verifier.UseSourceFileRelativeDirectory("Snapshots");
    }
}

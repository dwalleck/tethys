using System.Collections.Generic;
using System.Linq;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

namespace Tethys.MinimalEndpoints.ImprovedSourceGenerators.Tests.Helpers;

public static class TestCompilationHelper
{
    public static CSharpCompilation CreateCompilation(string source)
    {
        var syntaxTree = CSharpSyntaxTree.ParseText(source);
        
        var references = new List<MetadataReference>
        {
            MetadataReference.CreateFromFile(typeof(object).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(System.Threading.Tasks.Task).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(System.Attribute).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(List<>).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(System.Enum).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(System.Guid).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(System.Linq.Enumerable).Assembly.Location),
        };

        // Add attribute definitions and mock ASP.NET Core types
        var attributesAndMocks = """
            using System;
            using System.Threading.Tasks;
            
            namespace Tethys.MinimalEndpoints
            {
                public enum HttpMethod
                {
                    Get, Post, Put, Delete, Patch, Head, Options
                }

                [AttributeUsage(AttributeTargets.Class)]
                public class EndpointAttribute : Attribute
                {
                    public EndpointAttribute(HttpMethod method, string pattern) { }
                }

                [AttributeUsage(AttributeTargets.Method)]
                public class HandlerAttribute : Attribute { }

                [AttributeUsage(AttributeTargets.Class)]
                public class EndpointMetadataAttribute : Attribute
                {
                    public string[]? Tags { get; set; }
                    public string? Name { get; set; }
                    public string? Summary { get; set; }
                    public string? Description { get; set; }
                    public bool RequiresAuthorization { get; set; }
                    public string[]? Policies { get; set; }
                    public string[]? Roles { get; set; }
                }

                public interface IEndpoint
                {
                    void MapEndpoint(Microsoft.AspNetCore.Routing.IEndpointRouteBuilder app);
                }
            }

            namespace Microsoft.AspNetCore.Http
            {
                public interface IResult { }
                public static class Results
                {
                    public static IResult Ok() => null!;
                    public static IResult Ok(object value) => null!;
                    public static IResult NotFound() => null!;
                    public static IResult NoContent() => null!;
                    public static IResult Created(string uri, object value) => null!;
                    public static IResult StatusCode(int statusCode) => null!;
                }
                
                public delegate Task RequestDelegate(HttpContext context);
                public class HttpContext { }
            }

            namespace Microsoft.AspNetCore.Routing
            {
                public interface IEndpointRouteBuilder { }
                
                public interface IEndpointConventionBuilder { }
                
                public class RouteHandlerBuilder : IEndpointConventionBuilder
                {
                    public RouteHandlerBuilder WithTags(params string[] tags) => this;
                    public RouteHandlerBuilder WithName(string name) => this;
                    public RouteHandlerBuilder WithSummary(string summary) => this;
                    public RouteHandlerBuilder WithDescription(string description) => this;
                    public RouteHandlerBuilder RequireAuthorization(params string[] policies) => this;
                    public RouteHandlerBuilder RequireAuthorization(Action<object> configurePolicy) => this;
                }
            }

            namespace Microsoft.AspNetCore.Builder
            {
                using Microsoft.AspNetCore.Http;
                using Microsoft.AspNetCore.Routing;
                
                public static class EndpointRouteBuilderExtensions
                {
                    public static RouteHandlerBuilder MapGet(this IEndpointRouteBuilder endpoints, string pattern, Delegate handler)
                        => new RouteHandlerBuilder();
                        
                    public static RouteHandlerBuilder MapPost(this IEndpointRouteBuilder endpoints, string pattern, Delegate handler)
                        => new RouteHandlerBuilder();
                        
                    public static RouteHandlerBuilder MapPut(this IEndpointRouteBuilder endpoints, string pattern, Delegate handler)
                        => new RouteHandlerBuilder();
                        
                    public static RouteHandlerBuilder MapDelete(this IEndpointRouteBuilder endpoints, string pattern, Delegate handler)
                        => new RouteHandlerBuilder();
                        
                    public static RouteHandlerBuilder MapPatch(this IEndpointRouteBuilder endpoints, string pattern, Delegate handler)
                        => new RouteHandlerBuilder();
                        
                    public static RouteHandlerBuilder MapMethods(this IEndpointRouteBuilder endpoints, string pattern, IEnumerable<string> httpMethods, Delegate handler)
                        => new RouteHandlerBuilder();
                }
            }
            
            namespace System
            {
                public class Action<T> { }
            }
            
            namespace System.Collections.Generic
            {
                public interface IEnumerable<T> { }
            }
            """;
        
        var attributesTree = CSharpSyntaxTree.ParseText(attributesAndMocks);
        
        return CSharpCompilation.Create(
            "TestAssembly",
            new[] { syntaxTree, attributesTree },
            references,
            new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));
    }

    public static List<(string FileName, string Content)> GetGeneratedFiles(Compilation compilation)
    {
        return compilation.SyntaxTrees
            .Where(tree => tree.FilePath.EndsWith(".g.cs"))
            .Select(tree => (tree.FilePath, tree.GetText().ToString()))
            .ToList();
    }
    
    public static IEnumerable<Diagnostic> GetErrorDiagnostics(IEnumerable<Diagnostic> diagnostics)
    {
        return diagnostics.Where(d => d.Severity == DiagnosticSeverity.Error);
    }
}
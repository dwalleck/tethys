using System;

namespace Stratify.MinimalEndpoints.Attributes;

/// <summary>
/// Marks a class as an endpoint that should be auto-generated
/// </summary>
[AttributeUsage(AttributeTargets.Class, Inherited = false)]
public sealed class EndpointAttribute : Attribute
{
    public string Pattern { get; }
    public HttpMethodType Method { get; }

    public EndpointAttribute(HttpMethodType method, string pattern)
    {
        Method = method;
        Pattern = pattern;
    }
}

/// <summary>
/// HTTP method types for compile-time usage in attributes.
/// </summary>
/// <remarks>
/// This custom enum is necessary because C# attributes can only accept compile-time constants
/// as constructor parameters. While ASP.NET Core provides Microsoft.AspNetCore.Http.HttpMethod,
/// it's a class with static properties (not constants), which cannot be used in attribute
/// constructors. This enum allows us to specify HTTP methods at compile-time in the
/// [Endpoint] attribute, and the source generator maps these to the appropriate ASP.NET Core
/// MapGet/MapPost/etc. methods at code generation time.
/// </remarks>
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

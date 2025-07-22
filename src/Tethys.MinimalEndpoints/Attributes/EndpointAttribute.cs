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
/// HTTP method types for compile-time usage in attributes
/// </summary>
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

using System;

namespace Stratify.MinimalEndpoints.Attributes;

/// <summary>
/// Provides metadata for an endpoint
/// </summary>
[AttributeUsage(AttributeTargets.Class, Inherited = false)]
public sealed class EndpointMetadataAttribute : Attribute
{
    public string[]? Tags { get; set; }
    public string? Name { get; set; }
    public string? Summary { get; set; }
    public string? Description { get; set; }
    public bool RequiresAuthorization { get; set; }
    public string[]? Policies { get; set; } // User-defined, so string[] is appropriate
    public string[]? Roles { get; set; } // User-defined, so string[] is appropriate
}

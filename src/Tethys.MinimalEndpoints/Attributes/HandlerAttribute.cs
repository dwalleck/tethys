using System;

namespace Tethys.MinimalEndpoints.Attributes;

/// <summary>
/// Marks a method as the handler for an endpoint
/// </summary>
[AttributeUsage(AttributeTargets.Method, Inherited = false)]
public sealed class HandlerAttribute : Attribute
{
}
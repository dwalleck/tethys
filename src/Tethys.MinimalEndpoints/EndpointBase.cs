using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Http;
using Microsoft.AspNetCore.Routing;

namespace Tethys.MinimalEndpoints;

/// <summary>
/// Base class for endpoints following the REPR pattern
/// </summary>
public abstract class EndpointBase : IEndpoint
{
    public abstract void MapEndpoint(IEndpointRouteBuilder app);
}

/// <summary>
/// Base class for endpoints with request and response types
/// </summary>
/// <typeparam name="TRequest">The request type</typeparam>
/// <typeparam name="TResponse">The response type</typeparam>
public abstract class EndpointBase<TRequest, TResponse> : EndpointBase
    where TRequest : class
{
    protected abstract string Pattern { get; }
    protected abstract HttpMethod Method { get; }
    protected virtual string[]? Tags => null;
    protected virtual string? Name => null;
    protected virtual string? Summary => null;
    protected virtual string? Description => null;

    public override void MapEndpoint(IEndpointRouteBuilder app)
    {
        var builder = Method switch
        {
            _ when Method == HttpMethod.Get => app.MapGet(Pattern, HandleAsync),
            _ when Method == HttpMethod.Post => app.MapPost(Pattern, HandleAsync),
            _ when Method == HttpMethod.Put => app.MapPut(Pattern, HandleAsync),
            _ when Method == HttpMethod.Delete => app.MapDelete(Pattern, HandleAsync),
            _ when Method == HttpMethod.Patch => app.MapPatch(Pattern, HandleAsync),
            _ => throw new NotSupportedException($"HTTP method {Method} is not supported")
        };

        ConfigureEndpoint(builder);
    }

    protected abstract Task<IResult> HandleAsync(TRequest request, CancellationToken cancellationToken = default);

    protected virtual void ConfigureEndpoint(RouteHandlerBuilder builder)
    {
        builder
            .WithTagsIfAny(Tags)
            .WithNameIfProvided(Name)
            .WithSummaryIfProvided(Summary)
            .WithDescriptionIfProvided(Description);
            
        OnConfigureEndpoint(builder);
    }
    
    /// <summary>
    /// Override to add custom endpoint configuration
    /// </summary>
    protected virtual void OnConfigureEndpoint(RouteHandlerBuilder builder) { }
}

/// <summary>
/// Base class for endpoints with only a response type (no request body)
/// </summary>
/// <typeparam name="TResponse">The response type</typeparam>
public abstract class EndpointBase<TResponse> : EndpointBase
{
    protected abstract string Pattern { get; }
    protected abstract HttpMethod Method { get; }
    protected virtual string[]? Tags => null;
    protected virtual string? Name => null;
    protected virtual string? Summary => null;
    protected virtual string? Description => null;

    public override void MapEndpoint(IEndpointRouteBuilder app)
    {
        var builder = Method switch
        {
            _ when Method == HttpMethod.Get => app.MapGet(Pattern, HandleAsync),
            _ when Method == HttpMethod.Post => app.MapPost(Pattern, HandleAsync),
            _ when Method == HttpMethod.Put => app.MapPut(Pattern, HandleAsync),
            _ when Method == HttpMethod.Delete => app.MapDelete(Pattern, HandleAsync),
            _ when Method == HttpMethod.Patch => app.MapPatch(Pattern, HandleAsync),
            _ => throw new NotSupportedException($"HTTP method {Method} is not supported")
        };

        ConfigureEndpoint(builder);
    }

    protected abstract Task<IResult> HandleAsync(CancellationToken cancellationToken = default);

    protected virtual void ConfigureEndpoint(RouteHandlerBuilder builder)
    {
        builder
            .WithTagsIfAny(Tags)
            .WithNameIfProvided(Name)
            .WithSummaryIfProvided(Summary)
            .WithDescriptionIfProvided(Description);
            
        OnConfigureEndpoint(builder);
    }
    
    /// <summary>
    /// Override to add custom endpoint configuration
    /// </summary>
    protected virtual void OnConfigureEndpoint(RouteHandlerBuilder builder) { }
}
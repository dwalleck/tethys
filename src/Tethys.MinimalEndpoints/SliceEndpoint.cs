using FluentValidation;
using FluentValidation.Results;
using Microsoft.AspNetCore.Http;
using Microsoft.Extensions.DependencyInjection;

namespace Tethys.MinimalEndpoints;

/// <summary>
/// Simplified base class for vertical slice endpoints
/// </summary>
public abstract class SliceEndpoint : EndpointBase
{
    protected static IResult ValidationProblem(ValidationResult result) =>
        Results.ValidationProblem(result.ToDictionary());
        
    protected static IResult Ok<T>(T value) => Results.Ok(value);
    protected static IResult Created<T>(string location, T value) => Results.Created(location, value);
    protected static IResult NoContent() => Results.NoContent();
    protected static IResult NotFound() => Results.NotFound();
    protected static IResult BadRequest(object? error = null) => Results.BadRequest(error);
    protected static IResult Conflict(object? error = null) => Results.Conflict(error);
}

/// <summary>
/// Base class for slice endpoints with automatic validation
/// </summary>
public abstract class SliceEndpoint<TRequest> : SliceEndpoint
    where TRequest : class
{
    protected async Task<ValidationResult?> ValidateAsync(
        TRequest request, 
        HttpContext context,
        CancellationToken cancellationToken = default)
    {
        var validator = context.RequestServices.GetService<IValidator<TRequest>>();
        if (validator is null) 
            return null;
            
        return await validator.ValidateAsync(request, cancellationToken);
    }
}
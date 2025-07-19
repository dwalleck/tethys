using FluentValidation;
using Microsoft.AspNetCore.Http;
using Microsoft.AspNetCore.Routing;

namespace Tethys.MinimalEndpoints;

/// <summary>
/// Base class for endpoints with validation support
/// </summary>
/// <typeparam name="TRequest">The request type</typeparam>
/// <typeparam name="TResponse">The response type</typeparam>
public abstract class ValidatedEndpointBase<TRequest, TResponse> : EndpointBase<TRequest, TResponse>
    where TRequest : class
{
    protected override async Task<IResult> HandleAsync(TRequest request, CancellationToken cancellationToken = default)
    {
        var validator = GetValidator();
        if (validator is not null)
        {
            var validationResult = await validator.ValidateAsync(request, cancellationToken);
            if (!validationResult.IsValid)
            {
                return Results.ValidationProblem(validationResult.ToDictionary());
            }
        }

        return await HandleValidatedAsync(request, cancellationToken);
    }

    protected abstract Task<IResult> HandleValidatedAsync(TRequest request, CancellationToken cancellationToken = default);
    
    protected virtual IValidator<TRequest>? GetValidator() => null;
}
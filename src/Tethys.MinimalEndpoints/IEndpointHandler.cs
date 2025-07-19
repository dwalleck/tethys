namespace Tethys.MinimalEndpoints;

/// <summary>
/// Defines a handler for processing endpoint requests
/// </summary>
/// <typeparam name="TRequest">The request type</typeparam>
/// <typeparam name="TResponse">The response type</typeparam>
public interface IEndpointHandler<TRequest, TResponse>
{
    Task<TResponse> HandleAsync(TRequest request, CancellationToken cancellationToken = default);
}

/// <summary>
/// Defines a handler for processing endpoint requests with no request body
/// </summary>
/// <typeparam name="TResponse">The response type</typeparam>
public interface IEndpointHandler<TResponse>
{
    Task<TResponse> HandleAsync(CancellationToken cancellationToken = default);
}
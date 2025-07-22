using Microsoft.AspNetCore.Routing;

namespace Stratify.MinimalEndpoints;

public interface IEndpoint
{
    void MapEndpoint(IEndpointRouteBuilder app);
}

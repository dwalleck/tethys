using Microsoft.AspNetCore.Routing;

namespace Tethys.MinimalEndpoints;

public interface IEndpoint
{
    void MapEndpoint(IEndpointRouteBuilder app);
}
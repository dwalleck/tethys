using Microsoft.AspNetCore.Http.HttpResults;
using Tethys.Api.Database;
using Tethys.MinimalEndpoints;

namespace Tethys.Api.Features.Projects;

public static class GetProject
{
    public record Request(Guid Id);

    public record Response(Guid Id, string Name, string Description);

    public class Endpoint : IEndpoint
    {
        public void MapEndpoint(IEndpointRouteBuilder app)
        {
            app.MapGet("/projects/{id}", Handler).WithTags("Projects");
        }
    }

    public static async Task<Results<Ok<Response>, NotFound>> Handler(Guid id, AppDbContext context)
    {
        var project = await context.Projects.FindAsync(id).ConfigureAwait(false);
        return project is null
            ? TypedResults.NotFound()
            : TypedResults.Ok(new Response(project.Id, project.Name, project.Description));
    }
}

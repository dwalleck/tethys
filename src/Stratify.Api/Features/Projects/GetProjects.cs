using Microsoft.AspNetCore.Http.HttpResults;
using Microsoft.EntityFrameworkCore;
using Stratify.Api.Database;
using Stratify.MinimalEndpoints;

namespace Stratify.Api.Features.Projects;

public static class GetProjects
{
    public record Response(Guid Id, string Name, string Description);

    public class Endpoint : IEndpoint
    {
        public void MapEndpoint(IEndpointRouteBuilder app)
        {
            app.MapGet("/projects", Handler).WithTags("Projects");
        }
    }

    public static async Task<Results<Ok<List<Response>>, NotFound>> Handler(AppDbContext context)
    {
        var projects = await context.Projects.ToListAsync().ConfigureAwait(false);
        return TypedResults.Ok(projects.Select(x => new Response(x.Id, x.Name, x.Description)).ToList());
    }
}

using Microsoft.AspNetCore.Http.HttpResults;
using Stratify.Api.Database;
using Stratify.MinimalEndpoints;

namespace Stratify.Api.Features.Projects;

public static class DeleteProject
{
    public class Endpoint : IEndpoint
    {
        public void MapEndpoint(IEndpointRouteBuilder app)
        {
            app.MapDelete("/projects/{id}", Handler).WithTags("Projects");
        }
    }

    public static async Task<Results<NoContent, NotFound>> Handler(Guid id, AppDbContext context)
    {
        var project = await context.Projects.FindAsync(id).ConfigureAwait(false);
        if (project is null)
        {
            return TypedResults.NotFound();
        }

        context.Projects.Remove(project);
        await context.SaveChangesAsync().ConfigureAwait(false);
        return TypedResults.NoContent();
    }
}

using Microsoft.AspNetCore.Http.HttpResults;
using FluentValidation;
using Stratify.Api.Database;
using Stratify.MinimalEndpoints;

namespace Stratify.Api.Features.Projects;

public static class UpdateProject
{
    public record Request(string Name, string Description);
    public record Response(Guid Id, string Name, string Description);

    public sealed class Validator : AbstractValidator<Request>
    {
        public Validator()
        {
            RuleFor(x => x.Name).NotEmpty();
            RuleFor(x => x.Description).NotEmpty();
        }
    }

    public class Endpoint : IEndpoint
    {
        public void MapEndpoint(IEndpointRouteBuilder app)
        {
            app.MapPut("/projects/{id}", Handler).WithTags("Projects");
        }
    }

    public static async Task<Results<Ok<Response>, NotFound>> Handler(Guid id, Request request, AppDbContext context)
    {
        var project = await context.Projects.FindAsync(id).ConfigureAwait(false);
        if (project is null)
        {
            return TypedResults.NotFound();
        }

        project.Name = request.Name;
        project.Description = request.Description;

        await context.SaveChangesAsync().ConfigureAwait(false);
        return TypedResults.Ok(new Response(project.Id, project.Name, project.Description));
    }
}

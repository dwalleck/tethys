using Stratify.Api.Database;
using Stratify.MinimalEndpoints;
using FluentValidation;

namespace Stratify.Api.Features.Projects;

public static class CreateProject
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

    public sealed class Endpoint : IEndpoint
    {
        public void MapEndpoint(IEndpointRouteBuilder app)
        {
            app.MapPost("/projects", Handler);
        }
    }

    public static async Task<IResult> Handler(Request request, AppDbContext context, IValidator<Request> validator)
    {
        var validationResult = await validator.ValidateAsync(request).ConfigureAwait(false);

        if (!validationResult.IsValid)
        {
            return Results.BadRequest(validationResult.Errors);
        }

        var project = new Project
        {
            Name = request.Name,
            Description = request.Description
        };

        context.Projects.Add(project);
        await context.SaveChangesAsync().ConfigureAwait(false);
        return TypedResults.Created($"/projects/{project.Id}");
    }
}

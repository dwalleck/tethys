using FluentValidation;
using Microsoft.AspNetCore.Http.HttpResults;
using Tethys.MinimalEndpoints.Attributes;

namespace Tethys.Api.Features.Examples;

[Endpoint(HttpMethodType.Post, "/todos")]
[EndpointMetadata(
    Tags = new[] { "Todos" },
    Summary = "Create a new todo item",
    RequiresAuthorization = true,
    Roles = new[] { "User", "Admin" }
)]
public partial class CreateTodo
{
    public record Request(string Title, string? Description, DateTime? DueDate);
    public record Response(Guid Id, string Title, string? Description, DateTime? DueDate, DateTime CreatedAt);

    public class Validator : AbstractValidator<Request>
    {
        public Validator()
        {
            RuleFor(x => x.Title).NotEmpty().MaximumLength(200);
            RuleFor(x => x.Description).MaximumLength(1000);
            RuleFor(x => x.DueDate).GreaterThan(DateTime.UtcNow).When(x => x.DueDate.HasValue);
        }
    }

    [Handler]
    public static async Task<Results<Created<Response>, ValidationProblem>> HandleAsync(
        Request request,
        IValidator<Request> validator,
        CancellationToken cancellationToken)
    {
        var validationResult = await validator.ValidateAsync(request, cancellationToken);
        if (!validationResult.IsValid)
        {
            return TypedResults.ValidationProblem(validationResult.ToDictionary());
        }

        var response = new Response(
            Guid.NewGuid(),
            request.Title,
            request.Description,
            request.DueDate,
            DateTime.UtcNow
        );

        return TypedResults.Created($"/todos/{response.Id}", response);
    }
}
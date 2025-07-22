using Microsoft.AspNetCore.Http;
using Stratify.MinimalEndpoints;
using Stratify.MinimalEndpoints.Attributes;

namespace Stratify.ImprovedSourceGenerators.IntegrationTests.TestEndpoints;

[Endpoint(HttpMethodType.Post, "/api/todos")]
[EndpointMetadata(
    RequiresAuthorization = true,
    Policies = new[] { "TodoWritePolicy" }
)]
public partial class CreateTodoEndpoint
{
    private static readonly List<Todo> _todos = new();

    [Handler]
    public static Task<IResult> HandleAsync(CreateTodoRequest request)
    {
        var todo = new Todo
        {
            Id = _todos.Count + 1,
            Title = request.Title,
            IsComplete = false
        };

        _todos.Add(todo);

        return Task.FromResult(Results.Created($"/api/todos/{todo.Id}", todo));
    }
}

public record CreateTodoRequest(string Title);

public class Todo
{
    public int Id { get; set; }
    public string Title { get; set; } = "";
    public bool IsComplete { get; set; }
}

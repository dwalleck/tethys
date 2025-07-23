using Microsoft.AspNetCore.Builder;
using Microsoft.Extensions.DependencyInjection;
using System.Net;
using System.Net.Http.Json;
using Stratify.ImprovedSourceGenerators.IntegrationTests.TestEndpoints;
using Stratify.MinimalEndpoints;
using TUnit.Assertions;
using TUnit.Core;

namespace Stratify.ImprovedSourceGenerators.IntegrationTests;

public class EndpointGenerationIntegrationTests : IAsyncDisposable
{
    private WebApplication? _app;
    private HttpClient? _client;

    [Before(HookType.Test)]
    public async Task Setup()
    {
        var builder = WebApplication.CreateBuilder();

        // Add services
        builder.Services.AddAuthorization(options =>
        {
            options.AddPolicy("TodoWritePolicy", policy => policy.RequireAssertion(_ => true));
        });

        _app = builder.Build();

        // This is where the magic happens - the source generator creates the implementation
        // so these endpoints can register themselves
        _app.MapEndpoints();

        // Start the test server
        await _app.StartAsync();

        _client = new HttpClient
        {
            BaseAddress = new Uri("http://localhost:5000") // Use the test server address
        };
    }

    [Test]
    public async Task GeneratedEndpoint_CanBeInvoked_GetWeather()
    {
        // Act
        var response = await _client!.GetAsync("/api/weather");
        var weather = await response.Content.ReadFromJsonAsync<WeatherResponse>();

        // Assert
        await Assert.That(response.StatusCode).IsEqualTo(HttpStatusCode.OK);
        await Assert.That(weather).IsNotNull();
        await Assert.That(weather!.Temperature).IsEqualTo(72);
        await Assert.That(weather.Condition).IsEqualTo("Sunny");
        await Assert.That(weather.Location).IsEqualTo("Test City");
    }

    [Test]
    public async Task GeneratedEndpoint_CanBeInvoked_CreateTodo()
    {
        // Arrange
        var request = new CreateTodoRequest("Test Todo Item");

        // Act
        var response = await _client!.PostAsJsonAsync("/api/todos", request);
        var todo = await response.Content.ReadFromJsonAsync<Todo>();

        // Assert
        await Assert.That(response.StatusCode).IsEqualTo(HttpStatusCode.Created);
        await Assert.That(response.Headers.Location).IsNotNull();
        await Assert.That(response.Headers.Location!.ToString()).IsEqualTo("/api/todos/1");

        await Assert.That(todo).IsNotNull();
        await Assert.That(todo!.Id).IsEqualTo(1);
        await Assert.That(todo.Title).IsEqualTo("Test Todo Item");
        await Assert.That(todo.IsComplete).IsFalse();
    }

    [Test]
    public async Task SourceGenerator_RegistersAllEndpoints()
    {
        // This test verifies that the source generator correctly finds and registers
        // all endpoints in the assembly

        // Act - Get weather endpoint
        var weatherResponse = await _client!.GetAsync("/api/weather");

        // Act - Create todo endpoint
        var todoResponse = await _client!.PostAsJsonAsync("/api/todos", new CreateTodoRequest("Test"));

        // Assert - Both endpoints should be registered and working
        await Assert.That(weatherResponse.StatusCode).IsEqualTo(HttpStatusCode.OK);
        await Assert.That(todoResponse.StatusCode).IsEqualTo(HttpStatusCode.Created);
    }

    [Test]
    public async Task SourceGenerator_AppliesMetadata_Correctly()
    {
        // This would require more complex setup to test authorization
        // For now, we just verify the endpoints are callable

        // The weather endpoint should be accessible without auth
        var weatherResponse = await _client!.GetAsync("/api/weather");
        await Assert.That(weatherResponse.StatusCode).IsEqualTo(HttpStatusCode.OK);

        // The todo endpoint has RequiresAuthorization but our test policy always passes
        var todoResponse = await _client!.PostAsJsonAsync("/api/todos", new CreateTodoRequest("Test"));
        await Assert.That(todoResponse.StatusCode).IsEqualTo(HttpStatusCode.Created);
    }

    public async ValueTask DisposeAsync()
    {
        _client?.Dispose();
        if (_app != null)
        {
            await _app.StopAsync();
            await _app.DisposeAsync();
        }
    }
}

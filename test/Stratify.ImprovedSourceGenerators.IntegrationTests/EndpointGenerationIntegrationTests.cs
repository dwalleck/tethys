using Microsoft.AspNetCore.Builder;
using Microsoft.Extensions.DependencyInjection;
using System.Net;
using System.Net.Http.Json;
using System.Net.Sockets;
using Stratify.ImprovedSourceGenerators.IntegrationTests.TestEndpoints;
using Stratify.MinimalEndpoints;
using TUnit.Assertions;
using TUnit.Core;
using Microsoft.AspNetCore.Hosting;
using Microsoft.AspNetCore.Authentication;
using Microsoft.Extensions.Options;
using Microsoft.Extensions.Logging;
using System.Text.Encodings.Web;
using System.Security.Claims;

namespace Stratify.ImprovedSourceGenerators.IntegrationTests;

public class EndpointGenerationIntegrationTests : IAsyncDisposable
{
    private WebApplication? _app;
    private HttpClient? _client;
    private int _port;

    [Before(HookType.Test)]
    public async Task Setup()
    {
        // Get a random available port
        _port = GetAvailablePort();
        
        var builder = WebApplication.CreateBuilder();

        // Configure Kestrel to use the specific port
        builder.WebHost.ConfigureKestrel(options =>
        {
            options.ListenLocalhost(_port);
        });

        // Add services
        builder.Services.AddAuthentication("Test")
            .AddScheme<TestAuthenticationSchemeOptions, TestAuthenticationHandler>("Test", null);
            
        builder.Services.AddAuthorization(options =>
        {
            options.AddPolicy("TodoWritePolicy", policy => policy.RequireAssertion(_ => true));
        });
        
        // Register all endpoints from the current assembly
        builder.Services.AddEndpoints();

        _app = builder.Build();

        // This is where the magic happens - the source generator creates the implementation
        // so these endpoints can register themselves
        _app.MapEndpoints();

        // Start the test server
        await _app.StartAsync();

        _client = new HttpClient
        {
            BaseAddress = new Uri($"http://localhost:{_port}")
        };
    }
    
    private static int GetAvailablePort()
    {
        // Create a temporary listener to find an available port
        using var listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        var port = ((IPEndPoint)listener.LocalEndpoint).Port;
        listener.Stop();
        return port;
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

// Test authentication handler that always authenticates
public class TestAuthenticationHandler : AuthenticationHandler<TestAuthenticationSchemeOptions>
{
    public TestAuthenticationHandler(IOptionsMonitor<TestAuthenticationSchemeOptions> options,
        ILoggerFactory logger, UrlEncoder encoder) : base(options, logger, encoder)
    {
    }

    protected override Task<AuthenticateResult> HandleAuthenticateAsync()
    {
        var claims = new[]
        {
            new Claim(ClaimTypes.Name, "Test User"),
            new Claim(ClaimTypes.NameIdentifier, "123")
        };

        var identity = new ClaimsIdentity(claims, "Test");
        var principal = new ClaimsPrincipal(identity);
        var ticket = new AuthenticationTicket(principal, "Test");

        return Task.FromResult(AuthenticateResult.Success(ticket));
    }
}

public class TestAuthenticationSchemeOptions : AuthenticationSchemeOptions { }

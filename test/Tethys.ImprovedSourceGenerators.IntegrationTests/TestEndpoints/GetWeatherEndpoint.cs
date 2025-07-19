using Microsoft.AspNetCore.Http;
using Tethys.MinimalEndpoints;
using Tethys.MinimalEndpoints.Attributes;

namespace Tethys.ImprovedSourceGenerators.IntegrationTests.TestEndpoints;

[Endpoint(HttpMethodType.Get, "/api/weather")]
[EndpointMetadata(
    Name = "GetWeather",
    Summary = "Gets the current weather",
    Tags = new[] { "Weather" }
)]
public partial class GetWeatherEndpoint
{
    [Handler]
    public static async Task<IResult> HandleAsync()
    {
        await Task.Delay(10); // Simulate some async work
        
        var weather = new WeatherResponse
        {
            Temperature = 72,
            Condition = "Sunny",
            Location = "Test City"
        };
        
        return Results.Ok(weather);
    }
}

public record WeatherResponse
{
    public int Temperature { get; init; }
    public string Condition { get; init; } = "";
    public string Location { get; init; } = "";
}
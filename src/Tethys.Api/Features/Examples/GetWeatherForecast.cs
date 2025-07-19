using Microsoft.AspNetCore.Http.HttpResults;
using Tethys.MinimalEndpoints.Attributes;

namespace Tethys.Api.Features.Examples;

[Endpoint(HttpMethodType.Get, "/weather/{city}")]
[EndpointMetadata(
    Tags = new[] { "Weather" },
    Name = "GetWeatherForecast",
    Summary = "Gets weather forecast for a city",
    Description = "Returns the current weather forecast for the specified city"
)]
public partial class GetWeatherForecast
{
    public record Response(string City, int Temperature, string Summary);

    [Handler]
    public static async Task<Ok<Response>> HandleAsync(string city)
    {
        await Task.Delay(100); // Simulate work
        
        var temperature = Random.Shared.Next(-20, 55);
        var summaries = new[] { "Freezing", "Bracing", "Chilly", "Cool", "Mild", "Warm", "Balmy", "Hot", "Sweltering", "Scorching" };
        var summary = summaries[Random.Shared.Next(summaries.Length)];
        
        return TypedResults.Ok(new Response(city, temperature, summary));
    }
}
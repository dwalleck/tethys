# Tethys.MinimalEndpoints

A lightweight library for building vertical slice architecture with ASP.NET Core Minimal APIs.

## Overview

This library provides a simple, unopinionated foundation for organizing your ASP.NET Core Minimal API endpoints using a vertical slice architecture pattern.

## Core Concepts

### IEndpoint Interface

The foundation of the pattern - all endpoints implement this interface:

```csharp
public interface IEndpoint
{
    void MapEndpoint(IEndpointRouteBuilder app);
}
```

### Basic Usage

1. **Simple endpoint with static handler:**

```csharp
public static class GetWeatherForecast
{
    public record Request(string City);
    public record Response(string City, int Temperature, string Summary);

    public class Endpoint : IEndpoint
    {
        public void MapEndpoint(IEndpointRouteBuilder app)
        {
            app.MapGet("/weather/{city}", Handler)
                .WithTags("Weather");
        }
    }

    public static async Task<IResult> Handler(string city)
    {
        // Your logic here
        var response = new Response(city, Random.Shared.Next(-20, 55), "Sunny");
        return Results.Ok(response);
    }
}
```

2. **Using base classes for structure:**

```csharp
public class CreateProduct : EndpointBase<CreateProduct.Request, CreateProduct.Response>
{
    public record Request(string Name, decimal Price);
    public record Response(Guid Id, string Name, decimal Price);

    protected override string Pattern => "/products";
    protected override HttpMethod Method => HttpMethod.Post;
    protected override string[] Tags => ["Products"];

    protected override async Task<IResult> HandleAsync(Request request, CancellationToken cancellationToken)
    {
        // Your logic here
        var product = new Response(Guid.NewGuid(), request.Name, request.Price);
        return Results.Created($"/products/{product.Id}", product);
    }
}
```

3. **With validation:**

```csharp
public class UpdateProduct : ValidatedEndpointBase<UpdateProduct.Request, UpdateProduct.Response>
{
    public record Request(Guid Id, string Name, decimal Price);
    public record Response(Guid Id, string Name, decimal Price);

    public class Validator : AbstractValidator<Request>
    {
        public Validator()
        {
            RuleFor(x => x.Name).NotEmpty().MaximumLength(100);
            RuleFor(x => x.Price).GreaterThan(0);
        }
    }

    protected override string Pattern => "/products/{id}";
    protected override HttpMethod Method => HttpMethod.Put;
    protected override string[] Tags => ["Products"];

    protected override IValidator<Request>? GetValidator() => new Validator();

    protected override async Task<IResult> HandleValidatedAsync(Request request, CancellationToken cancellationToken)
    {
        // Your validated logic here
        return Ok(new Response(request.Id, request.Name, request.Price));
    }
}
```

## Registration

In your `Program.cs`:

```csharp
builder.Services.AddEndpoints(); // Discovers and registers all IEndpoint implementations

var app = builder.Build();

app.MapEndpoints(); // Maps all registered endpoints
```

## Source Generator Support

To reduce boilerplate even further, use the included source generator with attributes:

```csharp
[Endpoint(HttpMethodType.Get, "/weather/{city}")]
[EndpointMetadata(
    Tags = new[] { "Weather" },
    Summary = "Gets weather forecast for a city"
)]
public partial class GetWeatherForecast
{
    [Handler]
    public static async Task<Ok<WeatherData>> HandleAsync(string city)
    {
        // Your logic here
        return TypedResults.Ok(new WeatherData(city, 72, "Sunny"));
    }
}
```

The source generator will automatically implement `IEndpoint` and wire up all the metadata.

## Why This Pattern?

- **Vertical Slices**: Each feature is self-contained in a single file
- **Discoverable**: Endpoints are automatically discovered and registered
- **Testable**: Static handlers and clear separation make testing simple
- **Flexible**: Use as much or as little of the pattern as you need
- **No Magic**: Everything is explicit and easy to understand
- **Source Generators**: Optional code generation to reduce boilerplate
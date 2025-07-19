# Getting Started with Tethys Minimal Endpoints

This guide will walk you through implementing an API using Tethys Minimal Endpoints, a lightweight framework for building vertical slice architecture APIs in ASP.NET Core.

## Table of Contents
1. [Installation](#installation)
2. [Basic Concepts](#basic-concepts)
3. [Creating Your First Endpoint](#creating-your-first-endpoint)
4. [Endpoint Patterns](#endpoint-patterns)
5. [Validation](#validation)
6. [Advanced Features](#advanced-features)
7. [Project Organization](#project-organization)
8. [Migration from Controllers](#migration-from-controllers)

## Installation

Add the Tethys.MinimalEndpoints package to your project:

```xml
<PackageReference Include="Tethys.MinimalEndpoints" Version="1.0.0" />
```

If you want to use source generators for reduced boilerplate:
```xml
<PackageReference Include="Tethys.MinimalEndpoints.ImprovedSourceGenerators" Version="1.0.0" />
```

## Basic Concepts

Tethys Minimal Endpoints allows you to define API endpoints as self-contained classes. Each endpoint:
- Handles a single route/HTTP method combination
- Contains its request/response models
- Includes validation logic
- Is automatically discovered and registered

## Creating Your First Endpoint

### Method 1: Using Base Classes (Manual Approach)

```csharp
using Tethys.MinimalEndpoints;

namespace MyApi.Features.Products;

// Simple endpoint without request body
public class GetAllProductsEndpoint : EndpointBase<ProductsResponse>
{
    protected override string Pattern => "/api/products";
    protected override HttpMethod Method => HttpMethod.Get;
    
    protected override async Task<IResult> HandleAsync(CancellationToken ct)
    {
        var products = await GetProductsFromDatabase();
        return Ok(new ProductsResponse(products));
    }
}

public record ProductsResponse(List<Product> Products);
public record Product(int Id, string Name, decimal Price);
```

### Method 2: Using Attributes (Source Generator Approach)

```csharp
using Tethys.MinimalEndpoints;

namespace MyApi.Features.Products;

[Endpoint(HttpMethod.Get, "/api/products")]
[EndpointMetadata(
    Tags = new[] { "Products" },
    Summary = "Get all products",
    Description = "Returns a list of all available products"
)]
public partial class GetAllProducts
{
    [Handler]
    public static async Task<Ok<ProductsResponse>> HandleAsync(
        IProductRepository repository,
        CancellationToken ct)
    {
        var products = await repository.GetAllAsync(ct);
        return TypedResults.Ok(new ProductsResponse(products));
    }
}
```

## Endpoint Patterns

### 1. GET Endpoint with Route Parameters

```csharp
// Manual approach
public class GetProductByIdEndpoint : EndpointBase<int, ProductResponse>
{
    protected override string Pattern => "/api/products/{id}";
    protected override HttpMethod Method => HttpMethod.Get;
    
    protected override async Task<IResult> HandleAsync(int id, CancellationToken ct)
    {
        var product = await GetProductById(id);
        return product != null 
            ? Ok(new ProductResponse(product))
            : NotFound();
    }
}

// Attribute approach
[Endpoint(HttpMethod.Get, "/api/products/{id}")]
public partial class GetProductById
{
    [Handler]
    public static async Task<Results<Ok<ProductResponse>, NotFound>> HandleAsync(
        int id,
        IProductRepository repository,
        CancellationToken ct)
    {
        var product = await repository.GetByIdAsync(id, ct);
        return product != null 
            ? TypedResults.Ok(new ProductResponse(product))
            : TypedResults.NotFound();
    }
}
```

### 2. POST Endpoint with Request Body

```csharp
// Manual approach
public class CreateProductEndpoint : EndpointBase<CreateProductRequest, ProductResponse>
{
    protected override string Pattern => "/api/products";
    protected override HttpMethod Method => HttpMethod.Post;
    
    protected override async Task<IResult> HandleAsync(
        CreateProductRequest request, 
        CancellationToken ct)
    {
        var product = await CreateProduct(request);
        return Created($"/api/products/{product.Id}", new ProductResponse(product));
    }
}

// Attribute approach
[Endpoint(HttpMethod.Post, "/api/products")]
public partial class CreateProduct
{
    [Handler]
    public static async Task<Created<ProductResponse>> HandleAsync(
        CreateProductRequest request,
        IProductRepository repository,
        CancellationToken ct)
    {
        var product = await repository.CreateAsync(request, ct);
        return TypedResults.Created(
            $"/api/products/{product.Id}", 
            new ProductResponse(product));
    }
}

public record CreateProductRequest(string Name, decimal Price, string Description);
```

### 3. PUT/PATCH Endpoints

```csharp
[Endpoint(HttpMethod.Put, "/api/products/{id}")]
public partial class UpdateProduct
{
    [Handler]
    public static async Task<Results<NoContent, NotFound, ValidationProblem>> HandleAsync(
        int id,
        UpdateProductRequest request,
        IProductRepository repository,
        IValidator<UpdateProductRequest> validator,
        CancellationToken ct)
    {
        var validationResult = await validator.ValidateAsync(request, ct);
        if (!validationResult.IsValid)
            return TypedResults.ValidationProblem(validationResult.ToDictionary());
            
        var updated = await repository.UpdateAsync(id, request, ct);
        return updated 
            ? TypedResults.NoContent()
            : TypedResults.NotFound();
    }
}
```

### 4. DELETE Endpoint

```csharp
[Endpoint(HttpMethod.Delete, "/api/products/{id}")]
public partial class DeleteProduct
{
    [Handler]
    public static async Task<Results<NoContent, NotFound>> HandleAsync(
        int id,
        IProductRepository repository,
        CancellationToken ct)
    {
        var deleted = await repository.DeleteAsync(id, ct);
        return deleted 
            ? TypedResults.NoContent()
            : TypedResults.NotFound();
    }
}
```

## Validation

### Using ValidatedEndpointBase

```csharp
public class CreateProductEndpoint : ValidatedEndpointBase<CreateProductRequest, ProductResponse>
{
    protected override string Pattern => "/api/products";
    protected override HttpMethod Method => HttpMethod.Post;
    
    protected override async Task<IResult> HandleAsync(
        CreateProductRequest request, 
        CancellationToken ct)
    {
        // Validation happens automatically before this method is called
        var product = await CreateProduct(request);
        return Created($"/api/products/{product.Id}", new ProductResponse(product));
    }
}

// Define validator
public class CreateProductRequestValidator : AbstractValidator<CreateProductRequest>
{
    public CreateProductRequestValidator()
    {
        RuleFor(x => x.Name)
            .NotEmpty()
            .MaximumLength(100);
            
        RuleFor(x => x.Price)
            .GreaterThan(0)
            .LessThanOrEqualTo(999999.99m);
            
        RuleFor(x => x.Description)
            .MaximumLength(500);
    }
}
```

### Manual Validation with Attributes

```csharp
[Endpoint(HttpMethod.Post, "/api/products")]
public partial class CreateProduct
{
    [Handler]
    public static async Task<Results<Created<ProductResponse>, ValidationProblem>> HandleAsync(
        CreateProductRequest request,
        IValidator<CreateProductRequest> validator,
        IProductRepository repository,
        CancellationToken ct)
    {
        var validationResult = await validator.ValidateAsync(request, ct);
        if (!validationResult.IsValid)
            return TypedResults.ValidationProblem(validationResult.ToDictionary());
            
        var product = await repository.CreateAsync(request, ct);
        return TypedResults.Created($"/api/products/{product.Id}", new ProductResponse(product));
    }
}
```

## Advanced Features

### 1. Authorization

```csharp
[Endpoint(HttpMethod.Post, "/api/admin/products")]
[EndpointMetadata(
    RequiresAuthorization = true,
    Policies = new[] { "AdminOnly" },
    Roles = new[] { "Admin", "ProductManager" }
)]
public partial class CreateProductAdmin
{
    [Handler]
    public static async Task<Created<ProductResponse>> HandleAsync(
        CreateProductRequest request,
        IProductRepository repository,
        ClaimsPrincipal user,
        CancellationToken ct)
    {
        // Handler only executes if user passes authorization
        var product = await repository.CreateAsync(request, ct);
        return TypedResults.Created($"/api/products/{product.Id}", new ProductResponse(product));
    }
}
```

### 2. File Uploads

```csharp
[Endpoint(HttpMethod.Post, "/api/products/{id}/image")]
public partial class UploadProductImage
{
    [Handler]
    public static async Task<Results<NoContent, NotFound, BadRequest>> HandleAsync(
        int id,
        IFormFile image,
        IProductRepository repository,
        IFileService fileService,
        CancellationToken ct)
    {
        if (image.Length > 5_000_000) // 5MB limit
            return TypedResults.BadRequest("Image too large");
            
        var product = await repository.GetByIdAsync(id, ct);
        if (product == null)
            return TypedResults.NotFound();
            
        var imageUrl = await fileService.SaveAsync(image, ct);
        await repository.UpdateImageAsync(id, imageUrl, ct);
        
        return TypedResults.NoContent();
    }
}
```

### 3. Query Parameters and Filtering

```csharp
[Endpoint(HttpMethod.Get, "/api/products")]
public partial class SearchProducts
{
    [Handler]
    public static async Task<Ok<PagedResponse<ProductResponse>>> HandleAsync(
        [AsParameters] SearchProductsQuery query,
        IProductRepository repository,
        CancellationToken ct)
    {
        var result = await repository.SearchAsync(
            query.Search,
            query.MinPrice,
            query.MaxPrice,
            query.Page,
            query.PageSize,
            ct);
            
        return TypedResults.Ok(new PagedResponse<ProductResponse>(
            result.Items.Select(p => new ProductResponse(p)),
            result.TotalCount,
            query.Page,
            query.PageSize));
    }
}

public record SearchProductsQuery(
    string? Search = null,
    decimal? MinPrice = null,
    decimal? MaxPrice = null,
    int Page = 1,
    int PageSize = 20);
```

### 4. Response Caching

```csharp
[Endpoint(HttpMethod.Get, "/api/products/featured")]
[EndpointMetadata(
    Summary = "Get featured products",
    Description = "Returns cached list of featured products"
)]
public partial class GetFeaturedProducts
{
    [Handler]
    public static async Task<Ok<List<ProductResponse>>> HandleAsync(
        IProductRepository repository,
        IMemoryCache cache,
        CancellationToken ct)
    {
        var cacheKey = "featured-products";
        
        if (cache.TryGetValue<List<ProductResponse>>(cacheKey, out var cached))
            return TypedResults.Ok(cached!);
            
        var products = await repository.GetFeaturedAsync(ct);
        var response = products.Select(p => new ProductResponse(p)).ToList();
        
        cache.Set(cacheKey, response, TimeSpan.FromMinutes(5));
        
        return TypedResults.Ok(response);
    }
}
```

## Project Organization

### Recommended Folder Structure

```
MyApi/
├── Program.cs
├── Features/
│   ├── Products/
│   │   ├── CreateProduct.cs
│   │   ├── GetProduct.cs
│   │   ├── UpdateProduct.cs
│   │   ├── DeleteProduct.cs
│   │   ├── Models/
│   │   │   ├── ProductResponse.cs
│   │   │   └── CreateProductRequest.cs
│   │   └── Validators/
│   │       └── CreateProductRequestValidator.cs
│   ├── Orders/
│   │   ├── CreateOrder.cs
│   │   ├── GetOrder.cs
│   │   └── Models/
│   └── Users/
│       └── ...
├── Infrastructure/
│   ├── Database/
│   └── Services/
└── appsettings.json
```

### Program.cs Setup

```csharp
using Tethys.MinimalEndpoints;

var builder = WebApplication.CreateBuilder(args);

// Add services
builder.Services.AddEndpointsApiExplorer();
builder.Services.AddSwaggerGen();

// Add FluentValidation
builder.Services.AddValidatorsFromAssembly(Assembly.GetExecutingAssembly());

// Add your repositories and services
builder.Services.AddScoped<IProductRepository, ProductRepository>();

// Add Tethys endpoints
builder.Services.AddEndpoints(Assembly.GetExecutingAssembly());

var app = builder.Build();

// Configure pipeline
if (app.Environment.IsDevelopment())
{
    app.UseSwagger();
    app.UseSwaggerUI();
}

app.UseHttpsRedirection();
app.UseAuthorization();

// Map endpoints
app.MapEndpoints();

app.Run();
```

## Migration from Controllers

### Before (Controller)

```csharp
[ApiController]
[Route("api/[controller]")]
public class ProductsController : ControllerBase
{
    private readonly IProductRepository _repository;
    
    public ProductsController(IProductRepository repository)
    {
        _repository = repository;
    }
    
    [HttpGet("{id}")]
    public async Task<ActionResult<ProductResponse>> Get(int id)
    {
        var product = await _repository.GetByIdAsync(id);
        if (product == null)
            return NotFound();
            
        return Ok(new ProductResponse(product));
    }
    
    [HttpPost]
    public async Task<ActionResult<ProductResponse>> Create(CreateProductRequest request)
    {
        if (!ModelState.IsValid)
            return BadRequest(ModelState);
            
        var product = await _repository.CreateAsync(request);
        return CreatedAtAction(nameof(Get), new { id = product.Id }, new ProductResponse(product));
    }
}
```

### After (Tethys Minimal Endpoints)

```csharp
// GetProduct.cs
[Endpoint(HttpMethod.Get, "/api/products/{id}")]
public partial class GetProduct
{
    [Handler]
    public static async Task<Results<Ok<ProductResponse>, NotFound>> HandleAsync(
        int id,
        IProductRepository repository)
    {
        var product = await repository.GetByIdAsync(id);
        return product != null 
            ? TypedResults.Ok(new ProductResponse(product))
            : TypedResults.NotFound();
    }
}

// CreateProduct.cs
[Endpoint(HttpMethod.Post, "/api/products")]
public partial class CreateProduct
{
    [Handler]
    public static async Task<Results<Created<ProductResponse>, ValidationProblem>> HandleAsync(
        CreateProductRequest request,
        IValidator<CreateProductRequest> validator,
        IProductRepository repository)
    {
        var validationResult = await validator.ValidateAsync(request);
        if (!validationResult.IsValid)
            return TypedResults.ValidationProblem(validationResult.ToDictionary());
            
        var product = await repository.CreateAsync(request);
        return TypedResults.Created($"/api/products/{product.Id}", new ProductResponse(product));
    }
}
```

## Best Practices

1. **One Endpoint Per File**: Keep each endpoint in its own file for clarity
2. **Co-locate Related Code**: Keep request/response models near their endpoints
3. **Use Typed Results**: Leverage ASP.NET Core's typed results for better OpenAPI generation
4. **Inject Dependencies**: Use constructor or method injection as needed
5. **Handle Errors Gracefully**: Return appropriate HTTP status codes
6. **Validate Input**: Always validate request data before processing
7. **Use Cancellation Tokens**: Pass cancellation tokens through async operations

## Troubleshooting

### Common Issues

1. **Endpoints not discovered**: Ensure you're calling `AddEndpoints()` and `MapEndpoints()` in Program.cs
2. **Validation not working**: Register validators with DI using `AddValidatorsFromAssembly()`
3. **Source generator not running**: Clean and rebuild the solution
4. **Route conflicts**: Ensure each endpoint has a unique HTTP method + route combination

## Next Steps

- Explore the [Advanced Patterns](./ADVANCED_PATTERNS.md) guide
- Check out the [Sample Project](../samples/) for real-world examples
- Read about [Testing Endpoints](./TESTING.md)
- Learn about [Performance Optimization](./PERFORMANCE.md)
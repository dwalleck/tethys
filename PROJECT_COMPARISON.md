# Project Comparison: Tethys vs Similar Frameworks

This document provides a detailed comparison between Tethys Minimal Endpoints and similar frameworks in the .NET ecosystem.

## FastEndpoints

FastEndpoints is a mature, feature-rich framework for building REST APIs in ASP.NET Core using the REPR (Request-Endpoint-Response) pattern. It's one of the most comprehensive alternatives to traditional MVC controllers.

### Core Architecture Comparison

| Feature | Tethys | FastEndpoints |
|---------|--------|---------------|
| **Pattern** | REPR with source generators | REPR with reflection + optional source generators |
| **Endpoint Definition** | Attribute-based (`[Endpoint]`) | Fluent API in `Configure()` method + optional attributes |
| **Registration** | Compile-time via source generators | Runtime scanning + optional compile-time |
| **Base Classes** | `IEndpoint`, `EndpointBase<TReq,TRes>` | Similar base classes with more variants |
| **Validation** | FluentValidation integration | FluentValidation with enhanced features |

### Feature Gap Analysis

#### 1. Security & Authentication ❌ Major Gap

**FastEndpoints Provides:**
- **JWT Bearer Authentication**: Built-in `AddAuthenticationJwtBearer()` with token creation utilities
- **Cookie Authentication**: Native `AddAuthenticationCookie()` support
- **Multiple Auth Schemes**: Mix authentication methods per endpoint
- **Permission System**: Auto-generated permissions with `AccessControl()` attribute
  ```csharp
  AccessControl("Article_Create", behavior: Apply.ToThisEndpoint, groupNames: "Author", "Admin")
  ```
- **JWT Revocation**: Middleware for token blacklisting
- **CSRF Protection**: Built-in antiforgery token support
- **OAuth2 Scopes**: Validate scopes with `Scopes("read", "write")`

**Tethys Currently Lacks:** All of the above security features

#### 2. Middleware & Cross-Cutting Concerns ❌ Major Gap

**FastEndpoints Provides:**
- **Global Pre/Post Processors**: System-wide request/response interceptors
  ```csharp
  public class SecurityProcessor<TRequest> : IPreProcessor<TRequest>
  {
      public Task PreProcessAsync(IPreProcessorContext<TRequest> ctx, CancellationToken ct)
      {
          // Validate headers, short-circuit if needed
      }
  }
  ```
- **Processor State Sharing**: Share state between processors and endpoints
- **Command Bus**: Built-in mediator pattern with middleware pipeline
- **Exception Handling**: Default exception handler middleware
- **Rate Limiting**: Built-in throttling support
- **Response Caching**: Native output caching integration
- **Idempotency**: Handle duplicate requests gracefully

**Tethys Currently Lacks:** No middleware infrastructure beyond basic pre/post processors

#### 3. API Versioning ❌ Major Gap

**FastEndpoints Provides:**
- **Version Sets**: Group related endpoints
  ```csharp
  VersionSets.CreateApi("Orders", v => v
      .HasApiVersion(1.0)
      .HasApiVersion(2.0));
  ```
- **Header-Based Versioning**: `X-Api-Version` header support
- **URL Path Versioning**: `/v1/endpoint` style
- **Deprecation Support**: Mark endpoints as deprecated
- **Release Versioning**: Control visibility by release

**Tethys Currently Lacks:** No versioning support

#### 4. Advanced Binding & Validation ⚠️ Partial Gap

**FastEndpoints Provides:**
- **Custom Value Parsers**: Type-specific parsing logic
- **Complex Form Binding**: Deep object graphs from multipart forms
- **File Handling**: Advanced `IFormFile` binding and streaming
- **Unified Property Naming**: Consistent naming across all sources
- **Custom Binders**: Extend default binding behavior

**Tethys Has:**
- Basic model binding
- FluentValidation integration

**Tethys Lacks:**
- Advanced binding scenarios
- Custom parser registration
- Complex form handling

#### 5. Documentation & Client Generation ❌ Major Gap

**FastEndpoints Provides:**
- **Multiple Swagger Documents**: Separate docs for different API versions/groups
- **API Client Generation**: Auto-generate C# clients
  ```csharp
  app.MapApiClientEndpoint("/cs-client", c =>
  {
      c.Language = GenerationLanguage.CSharp;
      c.ClientClassName = "MyApiClient";
  });
  ```
- **Swagger Customization**: Fine-grained OpenAPI control
- **Endpoint Filtering**: Conditional documentation

**Tethys Currently Has:** Basic Swagger support only

#### 6. Testing Infrastructure ⚠️ Partial Gap

**FastEndpoints Provides:**
- **Integration Testing Framework**: 
  ```csharp
  public class MyTests(MyApp App) : TestBase<MyApp>
  {
      [Fact]
      public async Task Valid_User_Input()
      {
          var (rsp, res) = await App.Client.POSTAsync<Endpoint, Request, Response>(new());
      }
  }
  ```
- **Test Ordering**: Priority-based execution
- **Pre-configured Clients**: Shared HTTP clients with auth

**Tethys Has:** Basic xUnit integration tests

#### 7. Job Queues & Background Processing ❌ Not Planned

**FastEndpoints Provides:**
- **Persistent Job Queues**: Database-backed job storage
- **Progress Tracking**: Monitor long-running operations
- **Job Results**: Store and retrieve execution results

**Tethys:** Not in current scope

#### 8. Event System ❌ Not Planned

**FastEndpoints Provides:**
- **Event Handlers**: Pub/sub pattern
- **Event Hubs**: Remote event broker
- **Load Balancing**: Round-robin event distribution

**Tethys:** Not in current scope

#### 9. Advanced Features ⚠️ Mixed

**FastEndpoints Provides:**
- **Entity Mapping**: Built-in mapper pattern
  ```csharp
  public class PersonMapper : Mapper<Request, Response, Person>
  {
      public override Person ToEntity(Request r) => new() { ... };
      public override Response FromEntity(Person e) => new() { ... };
  }
  ```
- **Remote Procedure Calls**: gRPC-based RPC
- **Source Generator Optimizations**: Type discovery

**Tethys Has:**
- Source generator for endpoint registration
- Basic mapper pattern planned

**Tethys Lacks:**
- Advanced mapping features
- RPC support

#### 10. Configuration & Extensibility ⚠️ Partial Gap

**FastEndpoints Provides:**
- **Global Route Prefixes**: Apply to all endpoints
- **Endpoint Configurators**: Bulk endpoint configuration
- **Serializer Customization**: JSON options per endpoint
- **Endpoint Options**: Fine-grained behavior control

**Tethys Has:** Basic configuration through attributes

### Summary

**Tethys Strengths:**
- Simpler, more focused API
- Source generator-first approach
- Zero runtime reflection (when completed)
- Cleaner attribute-based configuration

**FastEndpoints Strengths:**
- Comprehensive feature set
- Production-ready with enterprise features
- Extensive middleware ecosystem
- Advanced security and versioning
- Rich testing infrastructure

**Recommendation:**
Tethys should focus on its core differentiators (simplicity, source generators, minimal overhead) rather than trying to match FastEndpoints' extensive feature set. Consider Tethys as a lightweight alternative for projects that don't need the full FastEndpoints feature set.

## Carter

Carter is a lightweight library that provides a thin layer of extension methods and functionality over ASP.NET Core Minimal APIs. It focuses on making code more explicit and enjoyable while maintaining simplicity.

### Core Architecture Comparison

| Feature | Tethys | Carter |
|---------|--------|--------|
| **Pattern** | REPR with source generators | Module-based with `ICarterModule` |
| **Endpoint Definition** | Attribute-based (`[Endpoint]`) | Module classes with `AddRoutes` method |
| **Registration** | Compile-time via source generators | Runtime with automatic discovery |
| **Base Classes** | `IEndpoint`, `EndpointBase<TReq,TRes>` | `ICarterModule` interface |
| **Philosophy** | Source generator-first, zero reflection | Thin layer over Minimal APIs |

### Feature Gap Analysis

#### 1. Core Module System ✅ Different Approach

**Carter Provides:**
- **Module Pattern**: Clean route organization
  ```csharp
  public class HomeModule : ICarterModule
  {
      public void AddRoutes(IEndpointRouteBuilder app)
      {
          app.MapGet("/", () => "Hello from Carter!");
      }
  }
  ```
- **Automatic Registration**: All `ICarterModule` implementations discovered
- **Direct Minimal API Access**: Full access to `IEndpointRouteBuilder`

**Tethys Approach:**
- Attribute-based endpoint definition
- Source generator registration
- More structured endpoint classes

#### 2. Validation ✅ Similar

**Carter Provides:**
- **FluentValidation Integration**: Built-in support
- **Extension Methods**: `Validate<T>()` and `ValidateAsync<T>()`
- **Simple Usage**: Integrated validation in route handlers

**Tethys Has:** Similar FluentValidation integration

#### 3. Dependency Injection ✅ Similar

**Carter Provides:**
- **Automatic Registration**: All implementations of `ICarterModule`, validators, and response negotiators
- **Manual Registration Option**: Via `CarterConfigurator`
  ```csharp
  builder.Services.AddCarter(configurator: c =>
  {
      c.WithModule<MyModule>();
      c.WithValidator<TestModelValidator>();
  });
  ```

**Tethys Has:** Similar DI integration with source generators

#### 4. Response Negotiation ⚠️ Partial Gap

**Carter Provides:**
- **Content Negotiation**: Via `IResponseNegotiator`
- **Default JSON Support**: Built-in JSON negotiation
- **Custom Negotiators**: Extensible negotiation system

**Tethys Currently Lacks:** Formal response negotiation system

#### 5. Security Features ❌ Major Gap

**Carter Provides:**
- **Basic Authorization**: Via Minimal API extensions
  ```csharp
  app.MapGet("/", () => "...").RequireAuthorization();
  ```
- **Leverages ASP.NET Core**: All standard security features

**Carter Lacks (similar to Tethys):**
- Built-in JWT handling
- Permission system
- Advanced authentication schemes

**Tethys:** Similar basic security support

#### 6. Middleware & Hooks ❌ Major Gap

**Carter Provides:**
- Standard ASP.NET Core middleware integration
- No specific before/after hooks

**Carter Lacks:**
- Global pre/post processors
- Request/response interceptors
- Middleware pipeline specific to endpoints

**Tethys:** Plans for basic pre/post processors

#### 7. OpenAPI/Swagger Support ❓ Unknown

**Carter:** No explicit OpenAPI support mentioned in core documentation
- Likely relies on ASP.NET Core's built-in OpenAPI support
- No custom documentation attributes

**Tethys Has:** Basic Swagger support planned

#### 8. Advanced Features ❌ Not Present

**Carter Lacks:**
- API versioning
- Client generation
- Job queues
- Event system
- Entity mapping patterns

**Tethys:** Similar limitations but with source generator focus

#### 9. File Handling ✅ Carter Advantage

**Carter Provides:**
- **File Upload Helpers**: `BindFile()` and `BindFiles()` extensions
- **File Saving**: Built-in file handling utilities

**Tethys Currently Lacks:** Dedicated file handling utilities

#### 10. Testing Support ❓ Basic

**Carter:** No specific testing framework mentioned
- Relies on standard ASP.NET Core testing approaches

**Tethys:** Basic xUnit integration planned

### Summary

**Carter Strengths:**
- Extremely lightweight and simple
- Direct access to Minimal APIs
- Clean module organization
- Minimal learning curve
- Good for simple APIs

**Carter Weaknesses:**
- Limited advanced features
- No built-in security enhancements
- No API versioning
- Limited middleware capabilities
- No source generator optimizations

**Tethys Differentiators:**
- Source generator-first approach
- Zero runtime reflection goal
- Attribute-based configuration
- More structured endpoint pattern

**Comparison:**
Carter and Tethys are more similar to each other than to FastEndpoints. Both aim for simplicity over features. Carter's module approach is more traditional, while Tethys's source generator approach is more innovative. Neither attempts to compete with FastEndpoints' enterprise features.

## Ardalis.ApiEndpoints

Ardalis.ApiEndpoints is one of the original libraries promoting the Request-Endpoint-Response (REPR) pattern as an alternative to MVC controllers. It emphasizes simplicity and SOLID principles.

### Core Architecture Comparison

| Feature | Tethys | Ardalis.ApiEndpoints |
|---------|--------|---------------------|
| **Pattern** | REPR with source generators | REPR with base class inheritance |
| **Endpoint Definition** | Attribute-based (`[Endpoint]`) | Base class with single `Handle()` method |
| **Registration** | Compile-time via source generators | Runtime MVC convention-based |
| **Base Classes** | `IEndpoint`, `EndpointBase<TReq,TRes>` | `EndpointBaseSync/Async` with fluent interfaces |
| **Philosophy** | Source generator-first, zero reflection | "Razor Pages for APIs" |

### Feature Gap Analysis

#### 1. Endpoint Definition Pattern ✅ Similar Philosophy

**Ardalis.ApiEndpoints Provides:**
- **Base Class Inheritance**: Clean endpoint structure
  ```csharp
  public class ListBooksEndpoint : EndpointBaseSync
      .WithoutRequest
      .WithResult<IList<BookDto>>
  {
      [HttpGet("/books")]
      public override IList<BookDto> Handle()
      {
          // Single responsibility - just list books
      }
  }
  ```
- **Fluent Generic Interfaces**: `.WithRequest<T>`, `.WithResult<T>`
- **Single Method Focus**: One `Handle()` method per endpoint

**Tethys Approach:**
- Similar REPR pattern
- Attribute-based instead of fluent interfaces
- Source generator registration vs runtime

#### 2. Base Class Options ✅ Different Approaches

**Ardalis.ApiEndpoints Provides:**
- `EndpointBaseSync`: Synchronous endpoints
- `EndpointBaseAsync`: Asynchronous endpoints  
- `EndpointBase`: Flexible base without strict generics
- Fluent combinations for request/response patterns

**Tethys Has:**
- `IEndpoint` interface
- `EndpointBase<TRequest, TResponse>`
- `ValidatedEndpointBase<TRequest, TResponse>`
- More traditional generic approach

#### 3. Routing & Documentation ✅ Similar

**Ardalis.ApiEndpoints:**
- Standard ASP.NET Core attributes (`[HttpGet]`, `[HttpPost]`)
- Swagger support via `[SwaggerOperation]`
- Tag-based endpoint grouping

**Tethys:** Similar attribute-based routing with OpenAPI support

#### 4. Validation ❌ Gap

**Ardalis.ApiEndpoints:**
- No built-in validation framework
- Relies on standard ASP.NET Core model validation
- Manual validation in `Handle()` method

**Tethys Has:** 
- Built-in FluentValidation integration
- `ValidatedEndpointBase` for automatic validation

#### 5. Dependency Injection ✅ Similar

**Ardalis.ApiEndpoints:**
- Constructor-based DI
- Standard ASP.NET Core DI container
- No special registration needed

**Tethys:** Similar DI support with automatic registration

#### 6. Security & Authorization ❌ Both Limited

**Ardalis.ApiEndpoints:**
- Uses standard ASP.NET Core authorization
- No additional security features
- Apply `[Authorize]` attributes as needed

**Tethys:** Similar basic security support

#### 7. Middleware & Filters ❌ Not Supported

**Ardalis.ApiEndpoints Lacks:**
- No endpoint-specific middleware
- No pre/post processors
- No filter pipeline

**Tethys Plans:** Basic pre/post processor support

#### 8. Advanced Features ❌ Minimal

**Ardalis.ApiEndpoints Lacks:**
- No source generators
- No response negotiation
- No versioning support
- No client generation
- No job queues or events

**Tethys Advantages:**
- Source generator optimization
- Planned mapper pattern
- Zero runtime reflection goal

#### 9. Testing ✅ Inherently Testable

**Ardalis.ApiEndpoints:**
- Endpoints are simple classes
- Easy to unit test `Handle()` method
- No special testing framework

**Tethys:** Similar testability with planned testing utilities

#### 10. Migration Path ✅ Clear Guidance

**Ardalis.ApiEndpoints Provides:**
- Detailed migration guide from controllers
- Clear philosophical stance: "MVC Controllers are an antipattern"
- Step-by-step conversion process

**Tethys:** Could benefit from similar migration guidance

### Summary

**Ardalis.ApiEndpoints Strengths:**
- Original REPR pattern implementation
- Extremely simple and focused
- Clear philosophical vision
- Mature and stable
- Great for learning REPR pattern

**Ardalis.ApiEndpoints Weaknesses:**
- Very minimal feature set
- No built-in validation
- No advanced features
- No source generator optimizations
- Limited extensibility

**Tethys Differentiators:**
- Source generator approach
- Built-in validation
- More base class options
- Zero runtime reflection goal
- More modern approach

**Comparison:**
Ardalis.ApiEndpoints is the philosophical predecessor to many REPR frameworks. It proves the concept but remains intentionally minimal. Tethys builds on these ideas with modern techniques (source generators) and additional features (validation), while FastEndpoints took the concept to enterprise scale. Ardalis.ApiEndpoints remains valuable for its simplicity and as a learning tool for the REPR pattern.

## Minimal APIs (Native ASP.NET Core)

*To be documented*
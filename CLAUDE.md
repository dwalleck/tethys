# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Tethys is a .NET 9.0 cloud-native API for test environment management, built using ASP.NET Core with .NET Aspire orchestration. The project follows a vertical slice architecture pattern.

## Key Commands

### Build and Run
```bash
# Build the solution
dotnet build

# Run tests
dotnet test

# Run a single test
dotnet test --filter "FullyQualifiedName~TestClassName.TestMethodName"

# Run the API directly
dotnet run --project src/Tethys.Api/Tethys.Api.csproj

# Run with .NET Aspire orchestration (recommended for development)
dotnet run --project src/Tethys.AppHost/Tethys.AppHost.csproj

# Build Docker image
docker build -f src/Tethys.Api/Dockerfile -t tethys-api .
```

### Development
```bash
# Watch mode for the API
dotnet watch --project src/Tethys.Api/Tethys.Api.csproj

# Format code
dotnet format

# Add a new migration (when using real database)
dotnet ef migrations add MigrationName --project src/Tethys.Api
```

## Architecture

### Vertical Slice Architecture
The codebase is organized by features rather than technical layers. Each feature contains all necessary components (endpoints, models, validators, handlers) in a single folder.

Structure:
- `src/Tethys.Api/Features/{FeatureName}/` - Contains all code for a specific feature
- Each operation (Create, Get, Update, Delete) is typically in its own file
- Endpoints are registered via the `IEndpoint` interface pattern

### Project Structure
- **Tethys.Api**: Main API project with feature slices
- **Tethys.AppHost**: .NET Aspire orchestration for local development
- **Tethys.ServiceDefaults**: Shared configuration for observability, health checks, and resilience
- **Tethys.Infrastructure**: Legacy shared code (being phased out during vertical slice migration)

### Key Patterns
1. **Endpoint Registration**: All endpoints implement `IEndpoint` and are auto-registered
2. **Validation**: FluentValidation for request validation
3. **Database**: Entity Framework Core with InMemory provider (development) and PostgreSQL support
4. **Observability**: OpenTelemetry integration via .NET Aspire
5. **Reusable Patterns**: The `Tethys.MinimalEndpoints` project contains reusable abstractions:
   - `IEndpoint` - Base interface for all endpoints
   - `EndpointExtensions` - Auto-registration helpers
   - `EndpointBase<TRequest, TResponse>` - Base class for endpoints with request/response
   - `ValidatedEndpointBase<TRequest, TResponse>` - Base class with built-in validation
   - `SliceEndpoint` - Simplified base class with helper methods

### Adding New Features
1. Create a new folder under `src/Tethys.Api/Features/{FeatureName}`
2. Implement endpoints using the `IEndpoint` interface
3. Add models, validators, and any feature-specific services in the same folder
4. Endpoints are automatically discovered and registered

### Database Context
The main DbContext is `TethysDbContext` located in `src/Tethys.Api/Database/`. Current entities:
- Project
- Environment (associated with Projects)

### Testing
Tests are located in `test/Tethys.Api.Tests/`. The project uses xUnit as the testing framework.
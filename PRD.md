# Tethys Minimal Endpoints - Product Requirements Document

## Overview

Tethys Minimal Endpoints is a lightweight, source generator-powered framework for building vertical slice architecture APIs in ASP.NET Core. Originally conceived as a test environment management API, the project has evolved into a reusable package that enables developers to organize their APIs using the REPR (Request-Endpoint-Response) pattern.

## Project Evolution

- **Original Goal**: Build a cloud-native API for test environment management
- **Current Direction**: Create a minimal endpoints framework similar to FastEndpoints, Carter, or ApiEndpoints
- **Key Pivot**: From building a specific API to building infrastructure that enables vertical slice architecture

## What We're Building

### Core Concept
A framework that allows developers to define API endpoints as self-contained classes, keeping all related code (request models, response models, validation, business logic) in a single location rather than scattered across multiple layers.

### Key Features

1. **Attribute-Based Endpoint Definition**
   - Use `[Endpoint]` attribute to define HTTP method and route pattern
   - Use `[Handler]` attribute to mark the method that handles requests
   - Use `[EndpointMetadata]` for OpenAPI metadata, authorization, etc.

2. **Source Generator Powered**
   - Compile-time code generation for zero-runtime discovery overhead
   - Automatic endpoint registration
   - Type-safe route parameter binding

3. **Vertical Slice Organization**
   - Each feature is self-contained in its own folder
   - No artificial layer separation (Controllers, Services, Models)
   - Improved maintainability and discoverability

4. **Integration with ASP.NET Core**
   - Works with minimal APIs
   - Supports all standard ASP.NET Core features (DI, middleware, etc.)
   - Compatible with OpenAPI/Swagger

## What We've Built So Far

### 1. Core Package (`src/Tethys.MinimalEndpoints`)
- **IEndpoint Interface**: Base contract for all endpoints
- **Endpoint Attributes**: 
  - `EndpointAttribute(HttpMethod method, string pattern)`
  - `HandlerAttribute`
  - `EndpointMetadataAttribute`
- **Base Classes**:
  - `EndpointBase<TRequest, TResponse>`: For endpoints with request/response
  - `ValidatedEndpointBase<TRequest, TResponse>`: With built-in FluentValidation
  - `SliceEndpoint`: Simplified base with helper methods
- **Extension Methods**: For auto-registration of endpoints

### 2. Source Generators (`src/Tethys.MinimalEndpoints.ImprovedSourceGenerators`)
- **EndpointGenerator**: Main generator that:
  - Discovers classes with `[Endpoint]` attribute
  - Extracts HTTP method from enum (identified issue with enum conversion)
  - Generates `IEndpoint` implementation with `MapEndpoint` method
  - Handles metadata attributes for OpenAPI, authorization, etc.

### 3. Test Suite (`test/Tethys.MinimalEndpoints.ImprovedSourceGenerators.Tests`)
- Comprehensive tests using TUnit framework
- Tests for enum handling (the suspected bug area)
- Tests for various endpoint patterns and scenarios

### 4. Example Implementation (`src/Tethys.Api`)
- Demonstrates vertical slice architecture
- Shows how to organize features (Projects, Environments)
- Integration with Entity Framework Core
- .NET Aspire orchestration

## Current Issues

1. **Enum Conversion Bug**: The `ExtractHttpMethod` method in `EndpointGenerator.cs` has issues converting enum values from attribute constructor arguments
2. **Generator Pattern Mismatch**: Tests were initially written for IEndpoint interface pattern, but the actual generator uses attribute-based pattern

## Comparison with Similar Frameworks

### FastEndpoints
- More feature-rich (includes validation, security, versioning)
- Larger API surface area
- Our approach is more minimal and focused

### Carter
- Module-based organization
- Runtime discovery
- We use compile-time generation for better performance

### ApiEndpoints
- Base class inheritance model
- Manual endpoint registration
- We provide automatic registration via source generators

## Future Roadmap

### Phase 1: Fix Current Issues ✓ (In Progress)
- [ ] Fix enum conversion bug in `ExtractHttpMethod`
- [ ] Update all tests to use attribute-based pattern
- [ ] Ensure all tests pass

### Phase 2: Core Features
- [ ] Request/Response binding improvements
- [ ] Better validation integration
- [ ] Enhanced metadata support
- [ ] Route constraint support

### Phase 3: Advanced Features
- [ ] Versioning support
- [ ] Rate limiting integration
- [ ] Authentication/Authorization helpers
- [ ] OpenAPI schema customization

### Phase 4: Tooling & Documentation
- [ ] Visual Studio templates
- [ ] CLI tooling for scaffolding
- [ ] Comprehensive documentation
- [ ] Migration guides from controllers

## Success Criteria

1. **Developer Experience**
   - Simple, intuitive API
   - Clear error messages
   - Minimal boilerplate

2. **Performance**
   - Zero runtime reflection
   - Minimal memory allocation
   - Fast startup time

3. **Compatibility**
   - Works with existing ASP.NET Core ecosystem
   - Supports latest .NET versions
   - Integrates with popular libraries

## Technical Decisions

1. **Source Generators over Reflection**: Better performance and AOT compatibility
2. **Attributes over Interfaces**: More flexible and allows partial class generation
3. **Minimal API Focus**: Aligned with modern ASP.NET Core direction
4. **Vertical Slice Architecture**: Proven pattern for maintainable APIs
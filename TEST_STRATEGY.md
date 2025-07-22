# Comprehensive Test Strategy for Stratify.MinimalEndpoints

## Overview

This document outlines a comprehensive testing strategy for the Stratify.MinimalEndpoints library and its source generators. The strategy is based on Andrew Lock's source generator testing guide and tailored to our specific needs.

## Test Project Structure

### Current Structure (To Be Refactored)

```
test/
├── Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests/  # Unit tests + duplicate snapshots
├── Stratify.ImprovedSourceGenerators.SnapshotTests/          # Dedicated snapshot tests
├── Stratify.ImprovedSourceGenerators.IntegrationTests/       # Integration tests
└── Stratify.Api.Tests/                                       # Legacy API tests (ignore)
```

### Proposed Structure

```
test/
├── Stratify.MinimalEndpoints.Tests/                          # Base library unit tests
├── Stratify.MinimalEndpoints.SourceGenerators.Tests/         # Generator unit tests
├── Stratify.MinimalEndpoints.SourceGenerators.SnapshotTests/ # Snapshot tests
├── Stratify.MinimalEndpoints.IntegrationTests/               # Integration tests
├── Stratify.MinimalEndpoints.PerformanceTests/               # Performance/cacheability tests
└── Stratify.MinimalEndpoints.NuGetTests/                     # NuGet package tests
```

## Testing Scope

### 1. Base Library (Stratify.MinimalEndpoints)

#### Components to Test

- **Attributes/**
  - `EndpointAttribute` - Constructor validation, property assignment
  - `EndpointMetadataAttribute` - All metadata properties
  - `HandlerAttribute` - Marker attribute behavior

- **Base Classes/**
  - `IEndpoint` - Interface contract
  - `EndpointBase<TRequest, TResponse>` - Request/response handling
  - `ValidatedEndpointBase<TRequest, TResponse>` - Validation integration
  - `SliceEndpoint` - Simplified base class functionality

- **Extensions/**
  - `EndpointExtensions` - Auto-registration logic
  - `RouteHandlerBuilderExtensions` - Metadata application

#### Test Types

- Unit tests for each component
- Integration tests for endpoint registration
- Example usage tests

### 2. Source Generators (Stratify.MinimalEndpoints.ImprovedSourceGenerators)

#### Components to Test

- **Models/**
  - `HttpMethod` enum
  - `EndpointClass` record
  - `EndpointMetadata` record
  - `HandlerMethod` record
  - `MethodParameter` record
  - `EquatableArray<T>` - Equality, GetHashCode, operators

- **Generators/**
  - `EndpointGeneratorImproved` - Main generation logic
  - `EndpointGenerator` (legacy) - If still needed

#### Test Categories

##### A. Unit Tests (Stratify.MinimalEndpoints.SourceGenerators.Tests)

1. **Model Tests**
   - Equality tests for all record types
   - EquatableArray functionality
   - HttpMethod enum handling

2. **Extraction Tests**
   - Attribute extraction from syntax
   - Metadata extraction
   - Handler method discovery
   - Parameter extraction

3. **Transformation Tests**
   - ISymbol to model conversion
   - Namespace resolution
   - Type hierarchy handling

4. **Error Handling Tests**
   - Missing partial keyword
   - No handler method
   - Invalid patterns
   - Null symbol handling

##### B. Snapshot Tests (Stratify.MinimalEndpoints.SourceGenerators.SnapshotTests)

1. **Basic Generation**
   - Simple GET endpoint
   - POST with parameters
   - PUT with route parameters
   - DELETE endpoint
   - Multiple endpoints in one file

2. **Metadata Scenarios**
   - Basic metadata (Name, Summary, Description)
   - Authorization metadata
   - Complex metadata with arrays
   - String escaping in metadata
   - Empty/null metadata

3. **Handler Variations**
   - Synchronous handlers
   - Async Task handlers
   - Async Task<T> handlers
   - IResult return types
   - Complex parameter types
   - Optional parameters
   - Default parameter values

4. **Edge Cases**
   - Nested classes
   - Generic classes
   - Namespaces with dots
   - Special characters in strings
   - Very long method signatures

##### C. Integration Tests (Stratify.MinimalEndpoints.IntegrationTests)

1. **Compilation Tests**
   - Generated code compiles
   - No compilation warnings
   - Works with project references

2. **Runtime Tests**
   - Endpoints register correctly
   - Routes match expectations
   - Metadata applies correctly
   - Authorization works
   - Actual HTTP calls succeed

3. **Framework Integration**
   - Works with minimal APIs
   - Dependency injection
   - Middleware compatibility

##### D. Performance Tests (Stratify.MinimalEndpoints.PerformanceTests)

1. **Cacheability Tests** (Following Andrew Lock's Part 10)
   - ForAttributeWithMetadataName performance
   - Incremental compilation caching
   - Output caching verification
   - No unnecessary regeneration

2. **Benchmarks**
   - Generation time for various scenarios
   - Memory usage
   - Large project performance

3. **Tracking Tests**
   - Verify tracking names
   - Pipeline step caching
   - Minimal recompilation

##### E. NuGet Package Tests (Stratify.MinimalEndpoints.NuGetTests)

1. **Package Installation**
   - Clean install works
   - Analyzer registration
   - No missing dependencies

2. **Multi-targeting**
   - Works with .NET 6, 7, 8, 9
   - Different C# language versions
   - Framework compatibility

## Test Implementation Guidelines

### 1. Snapshot Testing Setup

```csharp
// Module initializer
[ModuleInitializer]
public static void Init()
{
    VerifySourceGenerators.Enable();
    Verifier.UseDirectory("Snapshots");
}

// Test helper
public static SettingsTask Verify(string source, VerifySettings? settings = null)
{
    var compilation = CreateCompilation(source);
    var generator = new EndpointGeneratorImproved();
    GeneratorDriver driver = CSharpGeneratorDriver.Create(generator);
    driver = driver.RunGenerators(compilation);
    return Verifier.Verify(driver, settings);
}
```

### 2. Cacheability Testing Pattern

```csharp
[Test]
public void Generator_CachesOutput_WhenInputUnchanged()
{
    var source = "...";
    var compilation = CreateCompilation(source);

    var runResult1 = RunGeneratorWithTracking(compilation);
    var runResult2 = RunGeneratorWithTracking(compilation.Clone());

    AssertAllOutputsCached(runResult2);
}
```

### 3. Integration Testing Pattern

```csharp
[Test]
public async Task Endpoint_RegistersAndHandlesRequests()
{
    var builder = WebApplication.CreateBuilder();
    var app = builder.Build();

    app.MapEndpoints(); // Auto-registration

    using var client = app.GetTestClient();
    var response = await client.GetAsync("/api/test");

    await Assert.That(response.StatusCode).IsEqualTo(HttpStatusCode.OK);
}
```

## Test Data Organization

### 1. Test Sources

- Keep test source code in constants or separate files
- Group by scenario type
- Use realistic examples

### 2. Expected Outputs

- Store as verified snapshots
- Version control all verified files
- Review changes carefully

### 3. Test Fixtures

- Shared compilation setup
- Common references
- Reusable assertions

## CI/CD Integration

### 1. Test Execution

```yaml
- name: Run Unit Tests
  run: dotnet test test/Stratify.MinimalEndpoints.SourceGenerators.Tests

- name: Run Snapshot Tests
  run: dotnet test test/Stratify.MinimalEndpoints.SourceGenerators.SnapshotTests

- name: Run Integration Tests
  run: dotnet test test/Stratify.MinimalEndpoints.IntegrationTests

- name: Run Performance Tests
  run: dotnet test test/Stratify.MinimalEndpoints.PerformanceTests
```

### 2. Coverage Requirements

- Minimum 80% line coverage
- 100% coverage for Models
- 90% coverage for core generation logic
- Exclude generated code from coverage

### 3. Performance Gates

- Generation time < 100ms for typical project
- Zero cache misses for unchanged input
- Memory usage < 50MB

## Migration Plan

### Phase 1: Clean Up Current Tests

1. Remove duplicate snapshot tests from ImprovedSourceGenerators.Tests
2. Move unit tests to appropriate categories
3. Ensure all existing tests pass

### Phase 2: Fill Testing Gaps

1. Add missing model equality tests
2. Add cacheability tests
3. Add performance benchmarks
4. Add NuGet package tests

### Phase 3: Reorganize Projects

1. Create new test project structure
2. Move tests to appropriate projects
3. Update CI/CD configuration

### Phase 4: Documentation

1. Update test documentation
2. Create contributor guide
3. Document test patterns

## Success Metrics

1. **Coverage**: Achieve and maintain >80% code coverage
2. **Reliability**: Zero flaky tests
3. **Performance**: All performance tests pass gates
4. **Maintainability**: Clear test organization and naming
5. **Speed**: Full test suite runs in <2 minutes

## Key Testing Principles

1. **Test at the Right Level**: Unit test individual components, integration test the full pipeline
2. **Use Snapshot Testing**: For generated code verification
3. **Verify Cacheability**: Ensure incremental compilation works
4. **Test Error Scenarios**: Invalid input should produce helpful diagnostics
5. **Performance Matters**: Source generators run frequently, performance is critical
6. **Real-World Scenarios**: Test with realistic code examples

This comprehensive test strategy ensures the Stratify.MinimalEndpoints library and its source generators are thoroughly tested, performant, and maintainable.

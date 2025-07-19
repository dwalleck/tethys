# Code Coverage Plan: 90% Coverage Target

## Overview
This document outlines the plan to achieve 90% code coverage for both `Tethys.MinimalEndpoints` and `Tethys.MinimalEndpoints.ImprovedSourceGenerators` projects.

## Current Coverage Analysis

### Tethys.MinimalEndpoints
**Files to Cover:**
1. `IEndpoint.cs` - Interface (minimal testable code)
2. `EndpointExtensions.cs` - Critical registration logic
3. `EndpointBase.cs` - Abstract base implementations
4. `EndpointBase{T}.cs` - Generic endpoint bases
5. `ValidatedEndpointBase{TRequest,TResponse}.cs` - Validation logic
6. `SliceEndpoint.cs` - Helper methods
7. `Attributes/` - All attribute classes

**Key Test Scenarios Needed:**
- Endpoint registration and discovery
- HTTP method mapping
- Route parameter binding
- Request/response handling
- Validation flow
- Error handling
- Dependency injection

### Tethys.MinimalEndpoints.ImprovedSourceGenerators
**Files to Cover:**
1. `EndpointGenerator.cs` - Main generator logic
   - `Initialize()` - Generator setup
   - `IsPotentialEndpointClass()` - Syntax detection
   - `GetEndpointClassOrNull()` - Class analysis
   - `ExtractHttpMethod()` - Enum conversion (BUGGY)
   - `ExtractPattern()` - Route extraction
   - `FindHandlerMethod()` - Handler detection
   - `ExtractMetadata()` - Metadata processing
   - `GenerateEndpointImplementations()` - Code generation

**Key Test Scenarios Needed:**
- Valid endpoint class detection
- Invalid syntax handling
- All HTTP methods
- Complex route patterns
- Metadata extraction
- Authorization generation
- Edge cases and error conditions

## Testing Strategy

### Unit Test Structure

#### For Tethys.MinimalEndpoints
```csharp
test/Tethys.MinimalEndpoints.Tests/
├── EndpointExtensionsTests.cs
├── Base/
│   ├── EndpointBaseTests.cs
│   ├── EndpointBaseWithRequestTests.cs
│   ├── EndpointBaseWithResponseTests.cs
│   └── ValidatedEndpointBaseTests.cs
├── Attributes/
│   ├── EndpointAttributeTests.cs
│   ├── EndpointMetadataAttributeTests.cs
│   └── HandlerAttributeTests.cs
├── Integration/
│   ├── RegistrationTests.cs
│   ├── RoutingTests.cs
│   └── ValidationIntegrationTests.cs
└── Helpers/
    └── TestHelpers.cs
```

#### For Tethys.MinimalEndpoints.ImprovedSourceGenerators
```csharp
test/Tethys.MinimalEndpoints.ImprovedSourceGenerators.Tests/
├── EndpointGeneratorTests.cs
├── Scenarios/
│   ├── HttpMethodTests.cs
│   ├── RoutePatternTests.cs
│   ├── MetadataTests.cs
│   ├── AuthorizationTests.cs
│   ├── HandlerDetectionTests.cs
│   └── ErrorScenarioTests.cs
├── EdgeCases/
│   ├── EnumConversionTests.cs
│   ├── NestedClassTests.cs
│   ├── NamespaceTests.cs
│   └── SpecialCharacterTests.cs
├── Performance/
│   └── GeneratorPerformanceTests.cs
└── Helpers/
    └── SourceGeneratorTestHelper.cs
```

## Detailed Test Coverage Plan

### Phase 1: Base Package Tests (Tethys.MinimalEndpoints)

#### 1.1 EndpointExtensions Tests
```csharp
[Test] public async Task AddEndpoints_Should_Register_All_Endpoints()
[Test] public async Task AddEndpoints_Should_Skip_Abstract_Classes()
[Test] public async Task AddEndpoints_Should_Handle_Multiple_Assemblies()
[Test] public async Task MapEndpoints_Should_Call_MapEndpoint_On_All_Registered()
[Test] public async Task MapEndpoints_Should_Handle_Empty_Collection()
```

#### 1.2 EndpointBase Tests
```csharp
[Test] public async Task EndpointBase_Should_Map_Correct_HttpMethod()
[Test] public async Task EndpointBase_Should_Use_Correct_Pattern()
[Test] public async Task EndpointBase_Should_Handle_Cancellation()
[Test] public async Task EndpointBase_Should_Support_Dependency_Injection()
```

#### 1.3 Validation Tests
```csharp
[Test] public async Task ValidatedEndpoint_Should_Return_ValidationProblem_When_Invalid()
[Test] public async Task ValidatedEndpoint_Should_Call_Handler_When_Valid()
[Test] public async Task ValidatedEndpoint_Should_Use_Registered_Validator()
[Test] public async Task ValidatedEndpoint_Should_Handle_Missing_Validator()
```

### Phase 2: Source Generator Tests (ImprovedSourceGenerators)

#### 2.1 Core Generator Tests
```csharp
[Test] public async Task Generator_Should_Generate_IEndpoint_Implementation()
[Test] public async Task Generator_Should_Create_MapEndpoint_Method()
[Test] public async Task Generator_Should_Make_Class_Partial()
[Test] public async Task Generator_Should_Skip_Non_Endpoint_Classes()
```

#### 2.2 HTTP Method Tests (Focus on Enum Bug)
```csharp
[Test] public async Task Generator_Should_Extract_Get_Method()
[Test] public async Task Generator_Should_Extract_Post_Method()
[Test] public async Task Generator_Should_Extract_Put_Method()
[Test] public async Task Generator_Should_Extract_Delete_Method()
[Test] public async Task Generator_Should_Extract_Patch_Method()
[Test] public async Task Generator_Should_Handle_Unknown_Method()
[Test] public async Task Generator_Should_Handle_Enum_With_Explicit_Values()
[Test] public async Task Generator_Should_Handle_Enum_With_Different_Base_Type()
```

#### 2.3 Metadata Tests
```csharp
[Test] public async Task Generator_Should_Apply_Tags_From_Metadata()
[Test] public async Task Generator_Should_Apply_Summary_And_Description()
[Test] public async Task Generator_Should_Apply_Authorization_Requirements()
[Test] public async Task Generator_Should_Apply_Multiple_Policies()
[Test] public async Task Generator_Should_Apply_Roles()
[Test] public async Task Generator_Should_Escape_Special_Characters()
```

#### 2.4 Edge Cases
```csharp
[Test] public async Task Generator_Should_Handle_Nested_Classes()
[Test] public async Task Generator_Should_Handle_Generic_Classes()
[Test] public async Task Generator_Should_Handle_Classes_Without_Handler()
[Test] public async Task Generator_Should_Handle_Multiple_Handler_Methods()
[Test] public async Task Generator_Should_Handle_Special_Route_Characters()
```

### Phase 3: Integration Tests

#### 3.1 End-to-End Tests
```csharp
[Test] public async Task Generated_Endpoint_Should_Be_Discoverable()
[Test] public async Task Generated_Endpoint_Should_Handle_Requests()
[Test] public async Task Generated_Endpoint_Should_Apply_Authorization()
[Test] public async Task Generated_Endpoint_Should_Appear_In_OpenAPI()
```

## Code Coverage Tools Setup

### 1. Add Coverage Package
```xml
<PackageReference Include="coverlet.collector" Version="6.0.0">
  <PrivateAssets>all</PrivateAssets>
  <IncludeAssets>runtime; build; native; contentfiles; analyzers</IncludeAssets>
</PackageReference>
```

### 2. Run Coverage
```bash
# Run tests with coverage
dotnet test --collect:"XPlat Code Coverage" --results-directory ./coverage

# Generate report
dotnet tool install -g dotnet-reportgenerator-globaltool
reportgenerator -reports:coverage/**/coverage.cobertura.xml -targetdir:coverage/report -reporttypes:Html
```

### 3. Coverage Configuration
Create `.runsettings` file:
```xml
<?xml version="1.0" encoding="utf-8"?>
<RunSettings>
  <DataCollectionRunSettings>
    <DataCollectors>
      <DataCollector friendlyName="XPlat Code Coverage">
        <Configuration>
          <Format>cobertura</Format>
          <Exclude>[*]*.Generated.*,[*]*.g.cs</Exclude>
          <Include>[Tethys.*]*</Include>
          <ExcludeByAttribute>GeneratedCodeAttribute,CompilerGeneratedAttribute</ExcludeByAttribute>
          <SingleHit>false</SingleHit>
          <UseSourceLink>true</UseSourceLink>
          <IncludeTestAssembly>false</IncludeTestAssembly>
        </Configuration>
      </DataCollector>
    </DataCollectors>
  </DataCollectionRunSettings>
</RunSettings>
```

## Tracking Progress

### Coverage Targets by Component

| Component | Current | Target | Priority |
|-----------|---------|--------|----------|
| EndpointExtensions | 0% | 95% | HIGH |
| EndpointBase classes | 0% | 90% | HIGH |
| ValidatedEndpointBase | 0% | 95% | HIGH |
| Attributes | 0% | 85% | MEDIUM |
| EndpointGenerator.Initialize | 0% | 90% | HIGH |
| EndpointGenerator.ExtractHttpMethod | 0% | 100% | CRITICAL |
| EndpointGenerator.GenerateCode | 0% | 90% | HIGH |
| Edge Cases | 0% | 85% | MEDIUM |

### Untestable Code
Some code may be excluded from coverage:
- Attribute constructors (minimal logic)
- Generated code (excluded by configuration)
- Simple property getters/setters
- Interface definitions

## Success Metrics

1. **Overall Coverage**: ≥90% for both projects
2. **Critical Path Coverage**: 100% for core functionality
3. **Bug-Prone Areas**: 100% coverage for enum conversion
4. **Edge Cases**: ≥85% coverage
5. **Integration Tests**: Full end-to-end scenarios

## Timeline Integration

This coverage plan should be executed in parallel with the development plan:
- Phase 1-2 of development: Focus on unit tests (Days 1-4)
- Phase 3 of development: Add integration tests (Days 5-6)
- Continuous: Monitor coverage as fixes are implemented

## Next Steps

1. Set up coverage tooling in the project
2. Create test project structure
3. Begin with high-priority components
4. Track coverage metrics after each test session
5. Identify and document any untestable code
---
name: Source Generator Enhancement
about: Create a task for source generator improvements or fixes
title: '[TASK-XXX] '
labels: 'type: feature, component: source-generator'
assignees: ''

---

## Task: TASK-XXX - [Generator Enhancement]

**Type**: Feature/Bug Fix  
**Priority**: [P0/P1/P2]  
**Estimated**: [1-3 days]  
**Phase**: [0-5]

### Description
[What needs to be added/fixed in the source generator]

### Current Behavior
[How the generator currently works]

### Desired Behavior
[How it should work after this task]

### Generator Components Affected
- [ ] `EndpointGeneratorImproved.cs` - Main generator
- [ ] `EndpointClass.cs` - Model changes
- [ ] `EndpointMetadata.cs` - Metadata handling
- [ ] `HandlerMethod.cs` - Handler extraction
- [ ] `MethodParameter.cs` - Parameter handling
- [ ] Other: [specify]

### Implementation Details
- [ ] Syntax detection changes
- [ ] Code generation template updates
- [ ] Error handling improvements
- [ ] Performance optimizations

### Code Generation Example
```csharp
// Input code
[Endpoint(HttpMethodType.Post, "/api/example")]
public partial class ExampleEndpoint
{
    [Handler]
    public IResult Handle(Request request) => Results.Ok();
}

// Expected generated output
public partial class ExampleEndpoint : IEndpoint
{
    public void MapEndpoint(IEndpointRouteBuilder app)
    {
        app.MapPost("/api/example", Handle)
           .WithOpenApi();
    }
}
```

### Test Requirements
- [ ] Unit tests for generator logic
- [ ] Snapshot tests for generated code
- [ ] Integration tests with real compilation
- [ ] Edge case tests
- [ ] Cacheability tests (if applicable)

### Success Criteria
- [ ] Generator produces correct output
- [ ] No compilation errors in generated code
- [ ] Incremental compilation works correctly
- [ ] Generator performance acceptable
- [ ] All tests pass
- [ ] No regression in existing functionality

### Files to Modify
- `src/Tethys.MinimalEndpoints.ImprovedSourceGenerators/[file].cs`
- `test/Tethys.MinimalEndpoints.ImprovedSourceGenerators.Tests/[test].cs`
- `test/Tethys.ImprovedSourceGenerators.SnapshotTests/[snapshot].cs`

### Roslyn API Notes
- [Any specific Roslyn APIs to use]
- [Known issues or limitations]
- [Performance considerations]

### Verification
```bash
# Run generator tests
dotnet test test/Tethys.MinimalEndpoints.ImprovedSourceGenerators.Tests

# Run snapshot tests
dotnet test test/Tethys.ImprovedSourceGenerators.SnapshotTests

# Test in example project
dotnet build src/Tethys.Api
```

### References
- [Andrew Lock's Source Generator series](https://andrewlock.net/series/creating-a-source-generator/)
- [Microsoft Roslyn documentation](https://docs.microsoft.com/en-us/dotnet/csharp/roslyn-sdk/)
- `SOURCE_GENERATOR.md` in this repository
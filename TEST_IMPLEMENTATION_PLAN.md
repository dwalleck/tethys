# Test Implementation Plan for Tethys.MinimalEndpoints

Based on the TEST_STRATEGY.md, this document outlines the specific implementation plan with priorities and timelines.

## Current State Analysis

### What We Have
1. **Tethys.MinimalEndpoints.ImprovedSourceGenerators.Tests** (88 tests)
   - ✅ Metadata extraction tests (Phase 1.1) 
   - ✅ HTTP method coverage tests (Phase 1.1)
   - ✅ Parameter handling tests (Phase 1.3)
   - ✅ Error handling tests (Phase 1.4)
   - ✅ Model equality tests (Phase 1.5)
   - ❌ Duplicate/broken snapshot tests (need removal)
   - ⚠️ Coverage: 68.33%

2. **Tethys.ImprovedSourceGenerators.SnapshotTests** (17 tests)
   - ✅ Basic endpoint generation snapshots
   - ✅ Metadata scenarios
   - ✅ Working snapshot infrastructure
   - ❌ Missing cacheability tests

3. **Tethys.ImprovedSourceGenerators.IntegrationTests** (5 tests)
   - ✅ Basic integration test setup
   - ❌ Limited test scenarios

### Critical Issues to Fix
1. **Source generator not producing output** - Constructor argument order mismatch
2. **Namespace mismatch** - Tests using wrong namespace for attributes
3. **Duplicate snapshot tests** - Remove from main test project

## Implementation Phases

### Phase 0: Fix Critical Issues (1-2 days)
**Priority: CRITICAL**

1. **Fix Source Generator Output**
   - [ ] Fix constructor argument order in EndpointGeneratorImproved
   - [ ] Ensure ForAttributeWithMetadataName uses correct namespace
   - [ ] Verify generator produces output

2. **Clean Up Test Projects**
   - [ ] Remove snapshot tests from ImprovedSourceGenerators.Tests
   - [ ] Remove SimpleDebugTest.cs and DebugSnapshotTest.cs
   - [ ] Keep only unit tests in main test project

3. **Fix Test Infrastructure**
   - [ ] Update test helpers to use correct attribute namespaces
   - [ ] Ensure all tests use proper references

### Phase 1: Complete Existing Test Coverage (3-5 days)
**Priority: HIGH**

1. **Missing Unit Tests**
   - [ ] EquatableArray comprehensive tests
     - Equality with different scenarios
     - GetHashCode distribution
     - Operators (==, !=)
     - Implicit conversions
   - [ ] Model record equality tests
     - EndpointClass equality
     - EndpointMetadata with nulls
     - HandlerMethod variations
     - MethodParameter edge cases

2. **Additional Error Scenarios**
   - [ ] Multiple [Endpoint] attributes on same class
   - [ ] Abstract classes with [Endpoint]
   - [ ] Static classes with [Endpoint]
   - [ ] Invalid HTTP methods
   - [ ] Malformed patterns

3. **Base Library Tests**
   - [ ] IEndpoint interface usage
   - [ ] EndpointExtensions.MapEndpoints()
   - [ ] RouteHandlerBuilderExtensions

### Phase 2: Implement Cacheability Tests (2-3 days)
**Priority: HIGH**

Following Andrew Lock's Part 10 guide:

1. **Basic Cacheability**
   - [ ] Test unchanged input produces cached output
   - [ ] Test whitespace changes don't break cache
   - [ ] Test comment changes don't break cache

2. **Incremental Changes**
   - [ ] Adding new endpoint regenerates only new code
   - [ ] Changing one endpoint doesn't regenerate others
   - [ ] Metadata changes trigger appropriate regeneration

3. **Tracking Verification**
   - [ ] Add tracking names to generator pipeline
   - [ ] Verify each pipeline step caches correctly
   - [ ] Test with IncrementalGeneratorOutputKind tracking

### Phase 3: Expand Snapshot Tests (2-3 days)
**Priority: MEDIUM**

1. **Complex Scenarios**
   - [ ] Endpoints with 10+ parameters
   - [ ] Deeply nested namespaces
   - [ ] Generic type parameters in handlers
   - [ ] Arrays and collections in parameters
   - [ ] Nullable reference types

2. **Authorization Scenarios**
   - [ ] Multiple policies
   - [ ] Multiple roles
   - [ ] Mixed authorization attributes

3. **Special Cases**
   - [ ] Endpoints in partial classes across files
   - [ ] Endpoints with custom attributes
   - [ ] Unicode in strings/identifiers

### Phase 4: Performance Testing (2-3 days)
**Priority: MEDIUM**

1. **Benchmarks**
   - [ ] Create BenchmarkDotNet project
   - [ ] Measure generation time for 1, 10, 100 endpoints
   - [ ] Memory allocation tracking
   - [ ] Comparison with ISourceGenerator

2. **Large Project Simulation**
   - [ ] Generate test project with 500+ endpoints
   - [ ] Measure incremental compilation time
   - [ ] Verify memory usage stays reasonable

3. **Performance Gates**
   - [ ] Establish baseline metrics
   - [ ] Create failing tests for performance regression

### Phase 5: Integration Test Expansion (3-4 days)
**Priority: MEDIUM**

1. **HTTP Method Coverage**
   - [ ] Test all HTTP methods with actual calls
   - [ ] Route parameter binding
   - [ ] Query string parameters
   - [ ] Request body binding

2. **Dependency Injection**
   - [ ] Services in endpoint constructors
   - [ ] Scoped service resolution
   - [ ] ILogger injection

3. **Middleware Integration**
   - [ ] Authorization middleware
   - [ ] Custom middleware
   - [ ] Exception handling

### Phase 6: NuGet Package Testing (2-3 days)
**Priority: LOW**

1. **Package Creation**
   - [ ] Create test NuGet package
   - [ ] Verify analyzer registration
   - [ ] Test in isolated project

2. **Multi-targeting Tests**
   - [ ] .NET 6.0
   - [ ] .NET 7.0
   - [ ] .NET 8.0
   - [ ] .NET 9.0

3. **Compatibility Tests**
   - [ ] Different C# versions
   - [ ] Nullable reference types on/off
   - [ ] Different target frameworks

## Test Project Reorganization

### Step 1: Create New Structure
```bash
# Create new test projects
dotnet new classlib -n Tethys.MinimalEndpoints.Tests
dotnet new classlib -n Tethys.MinimalEndpoints.SourceGenerators.UnitTests
dotnet new classlib -n Tethys.MinimalEndpoints.SourceGenerators.PerformanceTests
dotnet new classlib -n Tethys.MinimalEndpoints.NuGetTests
```

### Step 2: Move Tests
1. Move model tests → SourceGenerators.UnitTests
2. Move extraction tests → SourceGenerators.UnitTests
3. Keep snapshot tests in existing project
4. Keep integration tests in existing project

### Step 3: Update References
1. Update project references
2. Update using statements
3. Fix namespace conflicts

## Success Criteria

### Phase 0 Complete When:
- [ ] All snapshot tests produce non-empty output
- [ ] No duplicate test code exists
- [ ] All existing tests pass

### Phase 1 Complete When:
- [ ] Code coverage > 80%
- [ ] All models have equality tests
- [ ] All error scenarios tested

### Phase 2 Complete When:
- [ ] Cacheability tests pass
- [ ] Tracking names implemented
- [ ] Performance verified

### Overall Project Complete When:
- [ ] All phases implemented
- [ ] Documentation updated
- [ ] CI/CD pipeline updated
- [ ] Coverage > 85%
- [ ] No flaky tests

## Timeline Summary

- **Week 1**: Phase 0 + Phase 1 (Fix critical issues, complete unit tests)
- **Week 2**: Phase 2 + Phase 3 (Cacheability + Snapshots)
- **Week 3**: Phase 4 + Phase 5 (Performance + Integration)
- **Week 4**: Phase 6 + Reorganization (NuGet + Cleanup)

Total estimated time: 4 weeks (with buffer)

## Next Steps

1. Start with Phase 0 immediately - fix the generator output issue
2. Remove duplicate snapshot tests
3. Begin systematic implementation following this plan
4. Track progress in TODO list
5. Update IMPROVE_TEST_COVERAGE.md as we progress
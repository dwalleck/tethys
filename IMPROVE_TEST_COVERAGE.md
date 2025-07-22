# Test Coverage Improvement Plan for Stratify.MinimalEndpoints.ImprovedSourceGenerators

## Current State Analysis

### Overall Coverage Metrics

- **Line Coverage**: 68.33% (up from ~45%)
- **Method Coverage**: ~80% (estimated, up from ~60%)
- **Branch Coverage**: Not measured (0%)
- **Test Framework**: TUnit v0.25.21
- **Total Tests**: 78 (all passing, up from 23)

### Test Type Distribution

- **Basic Functionality Tests**: 40+
- **Edge Case Tests**: 20+
- **Integration Tests**: 0
- **Snapshot Tests**: 0
- **Performance/Cacheability Tests**: 0
- **Error Handling Tests**: 12

## Coverage Gap Analysis

### 1. EndpointGenerator.cs (Original) - Estimated 40% Coverage

#### Well-Tested Areas

- ✅ Basic endpoint generation with [Endpoint] attribute
- ✅ HTTP method extraction from enum
- ✅ Pattern extraction
- ✅ Handler method discovery

#### Untested Areas

- ❌ Metadata extraction (`ExtractMetadata` - critical gap)
- ❌ String array extraction for tags, policies, roles
- ❌ Boolean value extraction
- ❌ String escaping for special characters
- ❌ Error paths (null symbols, invalid attributes)
- ❌ All HTTP methods (only GET/POST tested)

### 2. EndpointGeneratorImproved.cs - Estimated 30% Coverage

#### Well-Tested Areas

- ✅ Basic ForAttributeWithMetadataName usage
- ✅ Basic endpoint generation

#### Untested Areas

- ❌ `GetDefaultValueString()` - 0% coverage
- ❌ `ExtractMetadata()` - minimal coverage
- ❌ `ExtractStringArray()` - 0% coverage
- ❌ `ExtractBooleanValue()` - 0% coverage
- ❌ `EscapeString()` - 0% coverage
- ❌ Parameter handling with types and defaults
- ❌ Async/Task return type handling

### 3. Models.cs - Estimated 25% Coverage

#### Untested Areas

- ❌ `MethodParameter` - completely untested
- ❌ `HandlerMethod` - minimal coverage
- ❌ `EquatableArray<T>` equality implementation
- ❌ `EquatableArray<T>` hash code generation
- ❌ Implicit conversions

## Detailed Improvement Plan

### Phase 1: Add Missing Unit Tests (Target: 80% Coverage)

#### 1.1 Metadata Feature Tests (High Priority)

```csharp
// Test: EndpointMetadataExtractionTests.cs
- Test_Metadata_With_Tags
- Test_Metadata_With_Name_Summary_Description
- Test_Metadata_With_Authorization
- Test_Metadata_With_Policies
- Test_Metadata_With_Roles
- Test_Metadata_With_All_Properties
- Test_Metadata_String_Escaping
```

#### 1.2 HTTP Method Coverage (High Priority)

```csharp
// Test: HttpMethodCoverageTests.cs
- Test_Head_Method_Generation
- Test_Options_Method_Generation
- Test_Patch_Method_Generation
- Test_Unknown_Method_Defaults_To_Get
- Test_Invalid_Enum_Value_Handling
```

#### 1.3 Parameter Handling Tests (High Priority)

```csharp
// Test: HandlerParameterTests.cs
- Test_Handler_With_String_Parameter
- Test_Handler_With_Int_Parameter
- Test_Handler_With_Optional_Parameter
- Test_Handler_With_Default_Value_String
- Test_Handler_With_Default_Value_Number
- Test_Handler_With_Default_Value_Bool
- Test_Handler_With_Default_Value_Null
- Test_Handler_With_Complex_Type_Parameter
- Test_Handler_With_Multiple_Parameters
```

#### 1.4 Error Handling Tests (Medium Priority)

```csharp
// Test: ErrorHandlingTests.cs
- Test_Missing_Pattern_In_Attribute
- Test_Invalid_Pattern_Format
- Test_Missing_Handler_Method
- Test_Multiple_Handler_Methods
- Test_Null_Symbol_Handling
- Test_Invalid_Attribute_Arguments
```

#### 1.5 Model Equality Tests (Medium Priority)

```csharp
// Test: ModelEqualityTests.cs
- Test_EquatableArray_Equality
- Test_EquatableArray_Inequality
- Test_EquatableArray_HashCode
- Test_EquatableArray_Empty_Handling
- Test_EndpointClass_Equality
- Test_EndpointMetadata_Equality
- Test_HandlerMethod_Equality
- Test_MethodParameter_Equality
```

### Phase 2: Implement Best Practice Tests (Target: 90% Coverage)

#### 2.1 Snapshot Testing with Verify

```csharp
// Package: Verify.TUnit + Verify.SourceGenerators
// Test: EndpointGeneratorSnapshotTests.cs
- Snapshot_Basic_Endpoint
- Snapshot_Endpoint_With_Metadata
- Snapshot_Multiple_Endpoints
- Snapshot_Complex_Handler_Signatures
```

#### 2.2 Cacheability Tests

```csharp
// Test: GeneratorCacheabilityTests.cs
- Test_Unchanged_Input_Uses_Cached_Output
- Test_Changed_Attribute_Regenerates
- Test_Changed_Handler_Regenerates
- Test_Tracking_Names_Work_Correctly
- Test_Performance_With_Large_Compilation
```

#### 2.3 Integration Tests

```csharp
// New Project: Stratify.MinimalEndpoints.ImprovedSourceGenerators.IntegrationTests
- Test_Generator_As_Analyzer_Reference
- Test_Generated_Endpoints_Compile
- Test_Generated_Endpoints_Register_Correctly
- Test_Generated_Endpoints_Handle_Requests
```

### Phase 3: Advanced Testing (Target: 95%+ Coverage)

#### 3.1 Diagnostic Tests

```csharp
// Test: DiagnosticReportingTests.cs
- Test_Reports_Missing_Pattern_Diagnostic
- Test_Reports_Invalid_Method_Diagnostic
- Test_Reports_Missing_Handler_Diagnostic
- Test_Diagnostic_Locations_Correct
```

#### 3.2 Edge Case Tests

```csharp
// Test: EdgeCaseTests.cs
- Test_Unicode_In_Patterns
- Test_Special_Characters_In_Metadata
- Test_Very_Long_Strings
- Test_Deeply_Nested_Classes
- Test_Generic_Endpoint_Classes
- Test_Partial_Classes_Across_Files
```

#### 3.3 Performance Tests

```csharp
// Test: PerformanceTests.cs
- Test_Generator_Speed_With_100_Endpoints
- Test_Generator_Speed_With_1000_Endpoints
- Test_Memory_Usage_Reasonable
- Test_Incremental_Compilation_Benefits
```

## Implementation Strategy

### Week 1: Foundation (Phase 1.1-1.3)

- Add metadata extraction tests
- Complete HTTP method coverage
- Add parameter handling tests
- **Expected Coverage: 60% → 75%**

### Week 2: Robustness (Phase 1.4-1.5 + Phase 2.1)

- Add error handling tests
- Add model equality tests
- Implement snapshot testing
- **Expected Coverage: 75% → 85%**

### Week 3: Best Practices (Phase 2.2-2.3)

- Add cacheability tests
- Create integration test project
- **Expected Coverage: 85% → 90%**

### Week 4: Polish (Phase 3)

- Add diagnostic tests
- Add edge case coverage
- Add performance benchmarks
- **Expected Coverage: 90% → 95%+**

## Tooling Requirements

### 1. Test Dependencies to Add

```xml
<PackageReference Include="Verify.TUnit" Version="28.4.0" />
<PackageReference Include="Verify.SourceGenerators" Version="1.2.0" />
<PackageReference Include="coverlet.collector" Version="6.0.0" />
<PackageReference Include="ReportGenerator" Version="5.2.0" />
```

### 2. Coverage Collection Setup

```bash
# Run tests with coverage
dotnet test --collect:"XPlat Code Coverage" --results-directory ./TestResults

# Generate coverage report
reportgenerator -reports:./TestResults/*/coverage.cobertura.xml -targetdir:./CoverageReport -reporttypes:Html
```

### 3. CI/CD Integration

- Add coverage gates (minimum 80%)
- Generate coverage badges
- Fail builds if coverage drops

## Success Metrics

### Coverage Targets

- **Line Coverage**: 95%+ (from ~45%)
- **Branch Coverage**: 90%+ (from 0%)
- **Method Coverage**: 98%+ (from ~60%)

### Quality Metrics

- All edge cases tested
- All error paths covered
- Performance benchmarks established
- Integration tests passing

### Test Distribution Goals

- Unit Tests: 60%
- Integration Tests: 20%
- Snapshot Tests: 10%
- Performance Tests: 5%
- Error/Edge Cases: 5%

## Prioritized Action Items

1. **Immediate (Critical Gaps)**
   - [x] Test metadata extraction (COMPLETED - 10 tests)
   - [x] Test all HTTP methods (COMPLETED - 8 tests)
   - [x] Test parameter handling (COMPLETED - 15 tests)

2. **Short-term (This Week)**
   - [x] Add error handling tests (COMPLETED - 12 tests)
   - [ ] Implement snapshot testing (IN PROGRESS)
   - [x] Test model equality (COMPLETED - 13 tests)

3. **Medium-term (Next 2 Weeks)**
   - [ ] Create integration tests
   - [ ] Add cacheability tests
   - [ ] Set up coverage tracking

4. **Long-term (Month)**
   - [ ] Add diagnostic tests
   - [ ] Complete edge case coverage
   - [ ] Establish performance baselines

## Conclusion

The current test coverage of ~45% leaves significant gaps in critical functionality. By following this plan, we can achieve 95%+ coverage within a month, ensuring the source generator is robust, performant, and maintainable. The immediate priority is testing metadata extraction and parameter handling, as these are core features with zero coverage.

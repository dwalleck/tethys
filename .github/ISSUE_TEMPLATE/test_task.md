---
name: Testing Task
about: Create a task for adding or improving tests
title: '[TASK-XXX] Add tests for '
labels: 'type: testing'
assignees: ''

---

## Task: TASK-XXX - [Test Coverage for Component]

**Type**: Testing  
**Priority**: [P0/P1/P2/P3]  
**Estimated**: [2-4 hours / 1-2 days]  
**Phase**: [1-6]

### Description
Add comprehensive test coverage for [component/feature].

### Current Coverage
- Current: [XX%]
- Target: [80-100%]

### Test Scenarios to Cover
- [ ] Happy path: [describe]
- [ ] Edge case: [describe]
- [ ] Error case: [describe]
- [ ] Null/empty handling
- [ ] [Additional scenarios]

### Test Types Required
- [ ] Unit tests
- [ ] Integration tests (if applicable)
- [ ] Snapshot tests (if applicable)
- [ ] Performance benchmarks (if applicable)

### Files to Create/Modify
- `test/[Project]/[Component]Tests.cs` - Create comprehensive test suite
- `test/[Project]/TestHelpers/[Helper].cs` - Add any test utilities needed

### Test Pattern Example
```csharp
[Test]
public async Task MethodName_Scenario_ExpectedResult()
{
    // Arrange
    var sut = new ComponentUnderTest();
    
    // Act
    var result = await sut.MethodAsync();
    
    // Assert
    await Assert.That(result).IsNotNull();
    await Assert.That(result.Value).IsEqualTo(expected);
}
```

### Success Criteria
- [ ] All test scenarios implemented
- [ ] All tests pass
- [ ] Code coverage ≥ 80% for target component
- [ ] No flaky tests
- [ ] Tests follow TUnit patterns (not xUnit)
- [ ] Tests are maintainable and clear

### Notes
- Use TUnit for all tests (check context7 MCP for documentation)
- Follow existing test patterns in the codebase
- Ensure tests are deterministic and fast
- Use meaningful test names that describe scenario and expectation

### Verification
```bash
# Run tests
dotnet test test/[Project]

# Check coverage
dotnet test test/[Project] /p:CollectCoverage=true

# Run specific test
dotnet test --filter "FullyQualifiedName~[TestClass]"
```
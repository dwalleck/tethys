# TUnit Testing Framework Notes

## Key Differences from xUnit

### Attributes
- `[Test]` instead of `[Fact]`
- `[Arguments]` instead of `[InlineData]` 
- `[Arguments]` instead of `[Theory]` (TUnit automatically detects parameterized tests)
- `[BeforeEach]` instead of constructor
- `[AfterEach]` instead of Dispose
- `[BeforeAll]` and `[AfterAll]` for class-level setup/teardown

### Assertions
TUnit uses a fluent assertion API:
```csharp
// xUnit
Assert.Equal(expected, actual);
Assert.True(condition);
Assert.Empty(collection);

// TUnit
await Assert.That(actual).IsEqualTo(expected);
await Assert.That(condition).IsTrue();
await Assert.That(collection).IsEmpty();
```

### Async by Default
All TUnit tests and assertions are async:
```csharp
[Test]
public async Task MyTest()
{
    await Assert.That(true).IsTrue();
}
```

### Test Discovery
TUnit uses source generators for test discovery, which means:
- Tests are discovered at compile time
- Better performance
- May need to rebuild for new tests to appear

## Verify Integration

For snapshot testing with TUnit:
```xml
<PackageReference Include="Verify.TUnit" Version="28.4.0" />
```

The `[UsesVerify]` attribute works the same way, but tests use `[Test]` instead of `[Fact]`.
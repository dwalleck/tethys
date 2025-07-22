# Stratify Agent Quick Reference

## ⚠️ CRITICAL: MCP Tools

**MUST READ**: MCP-USAGE-GUIDE.md before starting any task!

**Always use context7 MCP for**:
- NuGet package documentation
- Method signatures and parameters
- Compilation errors with packages
- Current API examples

## Essential Commands

```bash
# Start your session
./scripts/dev-start.sh

# Check project status
./scripts/task-status.py

# Start a task
echo "Starting work on TASK-XXX" >> SESSION_NOTES.md

# Verify task completion
./scripts/verify-task.sh TASK-XXX

# Run tests with coverage
dotnet test /p:CollectCoverage=true

# Check documentation consistency
./scripts/check-consistency.py
```

## Key File Locations

**📋 Your Daily Files:**
- `AGENT-BOOTSTRAP.md` - Full workflow guide
- `SESSION_NOTES.md` - Your working notes
- `PR-WORKFLOW.md` - Pull request process
- `TEST-COVERAGE-PLAN.md` - All tasks with details
- `github-issues/` - Individual task files

**🏗️ Architecture:**
- `ARCHITECTURE.md` - System design & class diagrams
- `SOURCE_GENERATOR.md` - Generator implementation guide
- `TEST_STRATEGY.md` - Testing approach

**📁 Code Structure:**
```
src/
├── Stratify.MinimalEndpoints/        # Base library
├── Stratify.MinimalEndpoints.ImprovedSourceGenerators/  # Generators
└── Stratify.Api/                     # Example API
test/
├── *.ImprovedSourceGenerators.Tests/     # Unit tests
├── *.SnapshotTests/                      # Snapshot tests
└── *.IntegrationTests/                  # Integration tests
```

## Quick Workflow

1. **Start:** `./scripts/dev-start.sh`
2. **Pick task:** Check phases in TEST_IMPLEMENTATION_PLAN.md
3. **Branch:** `git checkout -b task-XXX-description`
4. **Check packages:** Use context7 MCP for any NuGet docs
5. **Code:** Follow TUnit patterns (always use TUnit!)
6. **Test:** Aim for 80%+ coverage
7. **Verify:** `./scripts/verify-task.sh TASK-XXX`
8. **PR:** `gh pr create --title "[TASK-XXX] Description" --body "Closes #XXX"`
9. **End:** Update SESSION_NOTES.md

## Where to Find What

| Need | Look In |
|------|----------|
| Task details | `github-issues/task-XXX-*.md` |
| Test patterns | `AGENT-BOOTSTRAP.md#testing` |
| API patterns | `ARCHITECTURE.md#usage-patterns` |
| Generator docs | `SOURCE_GENERATOR.md` |
| When stuck | `TEST_STRATEGY.md` |
| Coverage goals | `TEST_IMPLEMENTATION_PLAN.md` |

## Emergency Procedures

**Build broken?**
```bash
dotnet clean
dotnet restore
dotnet build
```

**Snapshot tests failing?**
```bash
cd test/Stratify.ImprovedSourceGenerators.SnapshotTests
rm -rf Snapshots/*.received.txt
dotnet test
```

**Lost context?**
1. Read SESSION_NOTES.md
2. Check git log
3. Run task-status.py

**TUnit syntax help?**
```bash
# Use context7 MCP to look up TUnit docs
# Step 1: mcp__context7__resolve-library-id --libraryName "TUnit"
# Step 2: mcp__context7__get-library-docs --context7CompatibleLibraryID "/tunit/tunit"
```

**Package compilation error?**
```bash
# Use context7 MCP immediately!
# Example for FluentValidation error:
# Step 1: mcp__context7__resolve-library-id --libraryName "FluentValidation"
# Step 2: mcp__context7__get-library-docs --context7CompatibleLibraryID "/fluentvalidation/fluentvalidation"
```

## Golden Rules

1. ✅ One task at a time
2. ✅ Test before marking complete
3. ✅ 80% coverage minimum
4. ✅ Update SESSION_NOTES.md
5. ✅ Commit every 30 minutes

## Pattern Quick Reference

**TUnit Test:**
```csharp
[Test]
public async Task MethodName_Scenario_ExpectedResult()
{
    // Arrange
    var sut = new ClassUnderTest();

    // Act
    var result = sut.Method();

    // Assert
    await Assert.That(result).IsEqualTo(expected);
}
```

**Snapshot Test:**
```csharp
[Test]
public Task GeneratesCorrectly()
{
    var source = @"[Endpoint(HttpMethodType.Get, ""/api/test"")]
                   public partial class TestEndpoint { }";

    return TestHelper.Verify(source);
}
```

**Generator Test:**
```csharp
[Test]
public async Task Generator_Scenario_ProducesExpectedOutput()
{
    // Arrange
    var compilation = CreateCompilation(source);

    // Act
    var result = RunGenerator(compilation);

    // Assert
    await Assert.That(result.GeneratedSources).HasCount().EqualTo(1);
}
```

## Test Project Mapping

| Test Type | Project | Framework |
|-----------|---------|-----------|
| Unit Tests | ImprovedSourceGenerators.Tests | TUnit |
| Snapshot Tests | SnapshotTests | Verify.TUnit |
| Integration | IntegrationTests | TUnit |
| API Tests | Api.Tests | xUnit (legacy) |

## Coverage Commands

```bash
# Quick coverage check
dotnet test /p:CollectCoverage=true

# Detailed HTML report
dotnet test /p:CollectCoverage=true /p:CoverletOutputFormat=opencover
reportgenerator -reports:"**/coverage.opencover.xml" -targetdir:"coveragereport"
open coveragereport/index.html

# Specific project coverage
dotnet test test/Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests /p:CollectCoverage=true
```

---
*Need more details? → AGENT-BOOTSTRAP.md*
*Stuck? → SOURCE_GENERATOR.md or TEST_STRATEGY.md*

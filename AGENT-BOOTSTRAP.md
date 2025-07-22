# Agent Bootstrap Guide for Stratify Development

## Quick Start for Agents

You are working on **Stratify Minimal Endpoints**, a source generator-powered framework for building vertical slice architecture APIs in ASP.NET Core. This document contains everything you need to understand the project, pick tasks, and complete them successfully.

### ⚠️ CRITICAL: MCP Tool Usage

**MANDATORY**: Read MCP-USAGE-GUIDE.md before starting any task. Always use context7 MCP tool for:

- Any compilation errors with external packages
- Before using any NuGet package APIs
- When unsure about method signatures or parameters
- To find current documentation and examples

### ⚠️ MANDATORY Daily Workflow

Every development session MUST follow this structured workflow to maintain momentum and avoid context loss.

## Daily Development Workflow

### Morning Startup (5 minutes)

```bash
# MANDATORY: Start every session with these steps
git pull origin main
python3 scripts/task-status.py
cat SESSION_NOTES.md | tail -20  # Review yesterday's session notes
# Check if any dependencies updated
# Clear mental space, focus on ONE task

# Check the daily checklist
cat DAILY_CHECKLIST.md
```

### Before Writing Code (2 minutes)

- [ ] Re-read the task requirements in TEST-COVERAGE-PLAN.md or github-issues/
- [ ] Check acceptance criteria
- [ ] Review relevant documentation (ARCHITECTURE.md, SOURCE_GENERATOR.md)
- [ ] Check MCP-USAGE-GUIDE.md if using external packages
- [ ] Use context7 for any NuGet package documentation needs
- [ ] Note the estimated time
- [ ] Create feature branch if needed: `git checkout -b task-XXX-description`

### While Coding (continuous)

- [ ] Write tests as you implement
- [ ] Keep methods under 50 lines
- [ ] Run tests frequently: `dotnet test`
- [ ] Commit after each small win: `git add . && git commit -m "test: [what you added]"`
- [ ] Check coverage periodically: `dotnet test /p:CollectCoverage=true`

### Decision Framework

When making technical decisions, ask:

1. Does this affect other tasks? → If yes, create change request
2. Is this reversible easily? → If no, document thoroughly
3. Is there a precedent in codebase? → If yes, follow it
4. Will this impact performance? → If yes, measure with benchmarks

### Before Lunch/Breaks

- [ ] Commit current work (even WIP): `git commit -am "WIP: [current state]"`
- [ ] Write quick note about next step in SESSION_NOTES.md
- [ ] Push to feature branch: `git push origin feature-branch`
- [ ] Note any blockers
- [ ] Stretch! 🙆‍♂️

### End of Session (10 minutes)

```bash
# MANDATORY: Complete these steps before ending work
dotnet test  # Run full test suite
./scripts/coverage-report.sh  # Check coverage

# Update session notes
cat >> SESSION_NOTES.md << EOF
## Session: $(date '+%Y-%m-%d %H:%M')
### Completed
- [What you finished]
### In Progress
- Current file: [filename:line]
- Next step: [specific action]
### Blockers
- [Any issues encountered]
### Time Spent
- Estimated: [from task]
- Actual: [your time]
EOF

git push origin feature-branch
./scripts/verify-task.sh TASK-XXX  # If task complete

# If task is complete and verified:
gh pr create --title "[TASK-XXX] Brief description" \
             --body "Closes #XXX" \
             --base main

# Plan tomorrow's first action
```

### Weekly Review (Fridays, 30 minutes)

- [ ] Run consistency check: `./scripts/check-consistency.py`
- [ ] Update LESSONS_LEARNED.md with insights
- [ ] Review week's progress in SESSION_NOTES.md
- [ ] Clean up feature branches
- [ ] Celebrate progress! 🎉
- [ ] Plan next week's goals

### Red Flags 🚩 - Stop and Reassess

- Stuck on same problem >1 hour → Check SOURCE_GENERATOR.md and Andrew Lock's guide
- Tests failing for >30 minutes → Review test strategy in TEST_STRATEGY.md
- Coverage dropping below 80% → Fix immediately
- Task taking 2x estimated time → Break down or seek help
- Changing code not in current task → Stop, create new task

### Quick Wins 🏆 - When Motivation is Low

- Add a missing test
- Fix a small TODO
- Improve an error message
- Update documentation
- Refactor a small method

## Essential Project Context

### What is Stratify Minimal Endpoints?

A .NET 9-based framework that:

- Uses source generators for compile-time endpoint registration
- Eliminates boilerplate with attribute-based development
- Provides zero-overhead abstractions
- Supports vertical slice architecture patterns

### Key Technologies You Must Use

```yaml
Core Stack:
  - .NET 9 with Minimal APIs
  - C# 12 with source generators
  - Roslyn for code generation

Testing:
  - TUnit (v0.25.21) for unit testing
  - Verify.TUnit for snapshot testing
  - Microsoft.CodeAnalysis.CSharp for generator tests

Quality:
  - FluentValidation for validation
  - Serilog for logging (in example API)
  - OpenTelemetry for observability
  - .NET Aspire for orchestration
```

## Project Structure

```
Stratify/
├── src/
│   ├── Stratify.MinimalEndpoints/              # Base library with attributes
│   ├── Stratify.MinimalEndpoints.ImprovedSourceGenerators/  # Source generators
│   ├── Stratify.Api/                          # Example API implementation
│   ├── Stratify.AppHost/                      # .NET Aspire orchestration
│   └── Stratify.ServiceDefaults/              # Shared configuration
├── test/
│   ├── Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests/  # Unit tests
│   ├── Stratify.ImprovedSourceGenerators.SnapshotTests/           # Snapshot tests
│   ├── Stratify.ImprovedSourceGenerators.IntegrationTests/        # Integration tests
│   └── Stratify.Api.Tests/                    # Example API tests
├── docs/                                     # Documentation
├── scripts/                                  # Utility scripts
└── github-issues/                           # Exported task issues
```

## How to Pick Your Next Task

### Task Selection Strategy

Apply this decision framework to maintain focus and momentum:

1. **Single Task Focus**: Complete one task fully before starting another
2. **Follow Dependencies**: Check TEST-COVERAGE-PLAN.md dependency order
3. **Priority Order**: P0 → P1 → P2 (never skip priorities)
4. **Avoid Analysis Paralysis**: If multiple valid options, pick the smallest

### 1. Check Task Dependencies

Look at TEST-COVERAGE-PLAN.md or TEST_IMPLEMENTATION_PLAN.md for the phase order:

- Phase 0 (Critical Fixes) must be done first
- Phase 1 (Core Tests) depends on Phase 0
- Phase 2 (Cacheability) depends on Phase 1
- etc.

### 2. Find Available Tasks

```bash
# Check current progress
python3 scripts/task-status.py

# Check which tasks are already completed
grep -n "TASK-" *.md | grep -i "completed"

# Find next P0 (highest priority) task
grep -A5 "Priority.*P0" TEST-COVERAGE-PLAN.md

# If stuck choosing, pick the one with clearest requirements
```

### 3. Task Priority Rules

- **P0**: Critical bugs blocking other work - do these first
- **P1**: Core functionality - do after P0s
- **P2**: Nice to have - do if all P0/P1 done

### 4. When to Defer vs Decide Now

**Decide Now (During Task)**:

- Test implementation details
- Private helper methods
- Test data builders
- Mock configurations

**Defer Decision (Create Change Request)**:

- Public API changes to generators
- New test frameworks or tools
- Pattern changes affecting multiple tasks
- Major refactoring

## Understanding Task Expectations

### Task Structure in Issues

Each task contains:

1. **Technical Details** - Specific implementation requirements
2. **Files to Modify** - Exact files and locations
3. **Code Examples** - Sample implementations
4. **Test Patterns** - Expected test structure
5. **Acceptance Criteria** - Checklist of completion requirements

### Definition of Done

A task is ONLY complete when:

- [ ] All acceptance criteria are met
- [ ] Code compiles without warnings
- [ ] All tests pass
- [ ] Code coverage ≥ 80% for new code
- [ ] Follows project patterns
- [ ] Documentation updated (if needed)
- [ ] Pull request created and linked to issue
- [ ] CI/CD passes on the PR
- [ ] PR approved (if working with team)

## Development Workflow

### Maintaining Development Rhythm

**Key Success Principles**:

- **Progress > Perfection**: One completed test is better than three partial tests
- **Red-Green-Refactor**: Write failing test, make it pass, then improve
- **Maintain Momentum**: If stuck for >30 mins, implement simplest solution first
- **Avoid Scope Creep**: Stick to task definition, note ideas for future tasks

### 1. Starting a Task

```bash
# Read the full task description first
cat github-issues/task-XXX-*.md

# Create a feature branch
git checkout -b task-XXX-description

# Navigate to the relevant project
cd test/[ProjectName]

# Build to verify
dotnet build
```

### 2. Project-Specific Commands

```bash
# Run all tests
dotnet test

# Run specific test project
dotnet test test/Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests

# Run tests with coverage
dotnet test /p:CollectCoverage=true /p:CoverletOutputFormat=opencover

# Run snapshot tests and update snapshots
dotnet test test/Stratify.ImprovedSourceGenerators.SnapshotTests -- -d verify.accept

# Run only unit tests (skip integration)
dotnet test --filter "Category!=Integration"

# Debug a specific test
dotnet test --filter "FullyQualifiedName~TestClassName.TestMethodName" --logger "console;verbosity=detailed"
```

### 3. Emergency Commands 🚨

```bash
# If build broken
git stash
git checkout main
dotnet build

# If snapshot tests failing
cd test/Stratify.ImprovedSourceGenerators.SnapshotTests
rm -rf Snapshots/*.received.txt
dotnet test

# If totally lost
git status
git diff
./scripts/task-status.py

# If need fresh start
git checkout main
git pull
git checkout -b fresh-start
```

### 5. Code Patterns to Follow

#### TUnit Test Pattern

```csharp
using TUnit.Core;
using TUnit.Assertions;
using TUnit.Assertions.Extensions;

namespace Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests;

public class EquatableArrayTests
{
    [Test]
    public async Task Equality_WithSameElements_ReturnsTrue()
    {
        // Arrange
        var array1 = new EquatableArray<string>(["a", "b", "c"]);
        var array2 = new EquatableArray<string>(["a", "b", "c"]);

        // Act & Assert
        await Assert.That(array1).IsEqualTo(array2);
        await Assert.That(array1 == array2).IsTrue();
    }
}
```

#### Snapshot Test Pattern

```csharp
[Test]
public Task GeneratesEndpointCorrectly()
{
    // Arrange
    var source = @"
using Stratify.MinimalEndpoints.Attributes;

[Endpoint(HttpMethodType.Get, ""/api/test"")]
public partial class TestEndpoint
{
    [Handler]
    public string Handle() => ""Hello"";
}";

    // Act & Assert
    return TestHelper.Verify(source);
}
```

#### Generator Test Pattern

```csharp
[Test]
public async Task ExtractHttpMethod_ValidEnum_ReturnsCorrectMethod()
{
    // Arrange
    var source = CreateCompilation(@"
[Endpoint(HttpMethodType.Post, ""/api/test"")]
public partial class TestEndpoint { }
");

    // Act
    var result = RunGenerator(source);

    // Assert
    await Assert.That(result.GeneratedSources).HasCount().EqualTo(1);
    await Assert.That(result.GeneratedSources[0].SourceText)
        .Contains("app.MapPost");
}
```

## Testing Requirements

### Code Coverage Requirements

**MANDATORY**: All code must maintain ≥ 80% coverage. Target 90% for critical components.

#### Coverage Rules

1. **New Code**: Must have ≥ 80% coverage before PR
2. **Modified Code**: Must maintain or improve coverage
3. **Generator Code**: Focus on transformation logic
4. **Models**: 100% coverage expected for equality/hash code

#### Running Coverage Analysis

```bash
# Run tests with coverage
dotnet test /p:CollectCoverage=true \
           /p:CoverletOutputFormat=opencover \
           /p:Exclude="[*]*.AssemblyInfo" \
           /p:ThresholdType=line \
           /p:Threshold=80

# Generate HTML report
reportgenerator -reports:"**/coverage.opencover.xml" \
                -targetdir:"coveragereport" \
                -reporttypes:Html

# View coverage report
open coveragereport/index.html
```

### What to Test

#### Must Test (100% Coverage Expected)

- **Model Classes**: All properties, equality, hash code
- **Generator Logic**: Extraction, transformation, generation
- **Utilities**: EquatableArray operations
- **Base Classes**: Public methods and properties

#### Should Test (80%+ Coverage)

- **Integration Points**: Generator with Roslyn
- **Edge Cases**: Null handling, empty collections
- **Error Paths**: Invalid input handling

#### Can Skip (With Justification)

- **Generated Code**: The output of generators
- **Attribute Classes**: Simple marker attributes
- **Experimental Code**: Marked as such

## Common Pitfalls to Avoid

### Technical Pitfalls

1. **Don't Skip Equality Tests** - Critical for incremental compilation
2. **Don't Ignore Cacheability** - Affects IDE performance
3. **Don't Hardcode Paths** - Use TestHelper methods
4. **Don't Skip Snapshot Review** - Always review generated code
5. **Don't Mix Test Types** - Keep unit/integration/snapshot separate
6. **Always Use TUnit** - This project uses TUnit for testing (use context7 MCP for current docs)

### Process Pitfalls

1. **Don't Skip Session Notes** - Context loss kills productivity
2. **Don't Ignore Blockers** - Document and seek help after 30 minutes
3. **Don't Change Out-of-Scope Code** - Create new task instead
4. **Don't Perfect First Version** - Meet requirements, refactor later
5. **Don't Skip Daily Workflow** - Consistency prevents drift
6. **Don't Work on Multiple Tasks** - Focus on one at a time

### Common Stalls and Solutions

- **Roslyn API Confusion**: Check SOURCE_GENERATOR.md examples
- **TUnit Syntax**: Use context7 MCP to look up current docs
- **Snapshot Failures**: Delete .received files and regenerate
- **Coverage Drop**: Add tests immediately, don't defer

## Key Files to Reference

| Need | File |
|------|------|
| Overall architecture | ARCHITECTURE.md |
| Source generator guide | SOURCE_GENERATOR.md |
| Test strategy | TEST_STRATEGY.md |
| Implementation plan | TEST_IMPLEMENTATION_PLAN.md |
| Task details | github-issues/task-*.md |
| Coverage progress | IMPROVE_TEST_COVERAGE.md |
| Daily workflow | DAILY_CHECKLIST.md |
| Context preservation | SESSION_NOTES.md |
| Past learnings | LESSONS_LEARNED.md |

## Success Metrics & Final Advice

### Track Your Progress

Monitor these metrics to stay on track:

**Development Metrics**:

- Tasks completed per week (target: 3-5)
- Test coverage trend (must stay ≥80%)
- Time to complete tasks vs estimates
- Build success rate (should be 100%)

**Quality Metrics**:

- Tests written per feature
- Snapshot tests reviewed
- Documentation completeness

**Personal Metrics**:

- Hours worked vs planned
- Motivation level (1-10 daily)
- Learning moments per week
- Number of times stuck >1 hour

### Remember

1. **Read the task completely** before starting
2. **Use exact patterns** from examples
3. **Follow TUnit conventions** - not xUnit
4. **Test everything** - maintain 80%+ coverage
5. **Update progress** - use session notes religiously
6. **Single task focus** - complete before moving on
7. **Progress > Perfection** - ship working tests

### Final Advice

- **Start Small, Think Big**: One test at a time
- **Use What You Build**: Run the framework to understand it
- **Document Surprises**: When reality differs from plan
- **Maintain Discipline**: Process exists to help, not hinder
- **Ask for Help**: When stuck >30 mins, articulate the problem clearly
- **Enjoy the Journey**: Building a framework is rewarding

> "Make it work, make it right, make it fast." - Kent Beck

This guide should enable you to work autonomously on Stratify testing tasks. Each task in the github-issues directory is self-contained with clear requirements. Follow the daily workflow, complete the acceptance criteria, and maintain momentum by focusing on one task at a time.

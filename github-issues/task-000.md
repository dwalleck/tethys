---
title: "[TASK-000] Remove FluentAssertions Due to Licensing"
labels: ["type: feature", "priority: p0", "phase: 0", "size: small"]
---

## Task: TASK-000 - Remove FluentAssertions Due to Licensing

**Type**: Feature  
**Priority**: P0 (Critical - Legal)  
**Estimated**: 1-2 hours  
**Phase**: 0

### Description
FluentAssertions has licensing restrictions. Remove from all test projects and replace with TUnit assertions.

### Dependencies
None

### Blocks
All testing tasks

### Success Criteria
- [ ] Remove FluentAssertions NuGet package from all projects
- [ ] Replace all FluentAssertions usages with TUnit assertions
- [ ] All existing tests still pass
- [ ] No build warnings about missing packages

### Files to Modify
- `test/Tethys.ImprovedSourceGenerators.SnapshotTests/Tethys.ImprovedSourceGenerators.SnapshotTests.csproj`
- `test/Tethys.ImprovedSourceGenerators.IntegrationTests/Tethys.ImprovedSourceGenerators.IntegrationTests.csproj`

### Implementation Notes
- Use TUnit for all tests (not xUnit)
- Follow existing patterns in the codebase
- Ensure 80%+ code coverage for new code
- Update documentation if APIs change

### Verification
Run `./scripts/verify-task.sh TASK-000` to verify completion.

---
_This issue was automatically generated from DEVELOPMENT-PLAN.md_

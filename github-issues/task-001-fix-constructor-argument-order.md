---
title: "[TASK-001] Fix Constructor Argument Order"
labels: ["type: bug", "priority: p0", "phase: 0", "size: small"]
---

## Task: TASK-001 - Fix Constructor Argument Order

**Type**: Bug
**Priority**: P0 (Critical)
**Estimated**: 2-4 hours
**Phase**: 0

### Description
The `EndpointAttribute` constructor takes `(HttpMethodType method, string pattern)` but the generator extracts them in the wrong order.

### Dependencies
None

### Blocks
All testing tasks

### Success Criteria
- [ ] Generator correctly extracts method at index 0
- [ ] Generator correctly extracts pattern at index 1
- [ ] All existing tests pass with fix
- [ ] Generated code uses correct HTTP method

### Files to Modify
- `src/Stratify.MinimalEndpoints.ImprovedSourceGenerators/EndpointGeneratorImproved.cs`

### Implementation Notes
- Use TUnit for all tests (not xUnit)
- Follow existing patterns in the codebase
- Ensure 80%+ code coverage for new code
- Update documentation if APIs change

### Verification
Run `./scripts/verify-task.sh TASK-001` to verify completion.

---
_This issue was automatically generated from DEVELOPMENT-PLAN.md_

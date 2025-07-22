---
title: "[TASK-002] Fix Namespace Inconsistencies"
labels: ["type: bug", "priority: p0", "phase: 0", "size: small"]
---

## Task: TASK-002 - Fix Namespace Inconsistencies

**Type**: Bug
**Priority**: P0 (Critical)
**Estimated**: 1-2 hours
**Phase**: 0

### Description
Test helpers create attributes in wrong namespace. Generator expects `Stratify.MinimalEndpoints.Attributes`.

### Dependencies
None

### Blocks
Model and equality tests

### Success Criteria
- [ ] All test helpers use correct namespace
- [ ] Generator finds attributes correctly
- [ ] No namespace-related test failures

### Implementation Notes
- Use TUnit for all tests (not xUnit)
- Follow existing patterns in the codebase
- Ensure 80%+ code coverage for new code
- Update documentation if APIs change

### Verification
Run `./scripts/verify-task.sh TASK-002` to verify completion.

---
_This issue was automatically generated from DEVELOPMENT-PLAN.md_

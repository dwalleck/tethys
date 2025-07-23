---
title: "[TASK-022] Auth/AuthZ Helpers"
labels: ["type: feature", "priority: p3 (Low)", "phase: 0", "size: large"]
---

## Task: TASK-022 - Auth/AuthZ Helpers

**Type**: Feature  
**Priority**: P3 (Low)  
**Estimated**: 2-3 days  
**Phase**: 0

### Description
Simplified authentication/authorization.

### Dependencies
TASK-014

### Blocks
None

### Success Criteria
- [ ] Auth attributes
- [ ] Policy helpers
- [ ] Claims extraction
- [ ] Testing utilities

### Implementation Notes
- Use TUnit for all tests (not xUnit)
- Follow existing patterns in the codebase
- Ensure 80%+ code coverage for new code
- Update documentation if APIs change

### Verification
Run `./scripts/verify-task.sh TASK-022` to verify completion.

---
_This issue was automatically generated from DEVELOPMENT-PLAN.md_

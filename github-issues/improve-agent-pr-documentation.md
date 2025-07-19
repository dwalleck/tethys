---
title: "Update agent documentation to make PR workflow more prominent"
labels: ["documentation", "developer-experience"]
---

## Description
The pull request creation step is not prominently featured in the agent documentation, making it easy for agents to miss this critical step after completing tasks.

## Current State
- PR creation is mentioned as step 8 in AGENT-QUICKSTART.md's Quick Workflow
- In AGENT-BOOTSTRAP.md, it's only in the "Verify Success" checklist
- PR-WORKFLOW.md has excellent detailed instructions but isn't linked from the bootstrap guide

## Proposed Changes
1. **AGENT-BOOTSTRAP.md**:
   - Add a dedicated "Creating Pull Requests" section after task completion
   - Make it clear that PR creation is MANDATORY after completing any task
   - Add prominent link to PR-WORKFLOW.md for detailed instructions
   - Include the basic `gh pr create` command in the main workflow

2. **AGENT-QUICKSTART.md**:
   - Move PR creation higher in the Quick Workflow list
   - Add emphasis that it's a required step

## Example Addition to AGENT-BOOTSTRAP.md

```markdown
### 6. Create Pull Request (MANDATORY)

**⚠️ IMPORTANT: You MUST create a pull request after completing any task. Never leave completed work on a feature branch without a PR.**

```bash
# Push your branch
git push origin task-XXX-description

# Create PR (see PR-WORKFLOW.md for detailed instructions)
gh pr create \
  --title "[TASK-XXX] Brief description" \
  --body "Closes #XXX" \
  --base main
```

For detailed PR guidelines, templates, and best practices, see [PR-WORKFLOW.md](./PR-WORKFLOW.md).
```

## Success Criteria
- [ ] Agents immediately know they must create a PR after task completion
- [ ] Clear link from AGENT-BOOTSTRAP.md to PR-WORKFLOW.md
- [ ] PR creation is treated as a mandatory step, not optional
- [ ] Basic PR command is visible without searching

## Priority
Medium - This will improve agent workflow efficiency and prevent incomplete task submissions

## Impact
- Reduces friction in agent workflow
- Ensures all completed work gets properly submitted
- Prevents agents from moving to next task without creating PR
- Improves overall development velocity
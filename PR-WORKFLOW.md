# Pull Request Workflow for Stratify Development

## Overview

This document defines the pull request workflow for Stratify development. All code changes MUST go through pull requests - never commit directly to main.

## Branch → PR → Merge Workflow

### 1. Start Work on an Issue

```bash
# Pick an issue from GitHub
# https://github.com/dwalleck/Stratify/issues

# Create feature branch
git checkout main
git pull origin main
git checkout -b task-XXX-brief-description

# Example: task-001-fix-constructor-order
```

### 2. Development Process

```bash
# Make changes following TDD
# Write test first
# Make it pass
# Refactor if needed

# Commit frequently with meaningful messages
git add .
git commit -m "test: add test for constructor argument order"
git commit -m "fix: correct constructor argument extraction in generator"

# Push to remote regularly
git push origin task-XXX-brief-description
```

### 3. Verify Task Completion

Before creating a PR, ensure:

```bash
# All tests pass
dotnet test

# Coverage is adequate (≥80%)
dotnet test /p:CollectCoverage=true

# No build warnings
dotnet build

# Run task verification
./scripts/verify-task.sh TASK-XXX
```

### 4. Create Pull Request

#### Option A: GitHub CLI (Recommended)

```bash
gh pr create \
  --title "[TASK-XXX] Brief description of changes" \
  --body "## Summary

  Brief description of what was changed and why.

  ## Changes
  - Fixed constructor argument order in EndpointGeneratorImproved
  - Added unit tests for argument extraction
  - Updated existing tests to match corrected behavior

  ## Testing
  - Added new unit tests
  - All existing tests pass
  - Coverage: 85%

  Closes #XXX" \
  --base main
```

#### Option B: GitHub Web UI

1. Go to https://github.com/dwalleck/Stratify
2. Click "Pull requests" → "New pull request"
3. Select your branch
4. Fill in the template

### 5. PR Title Format

Use this format for PR titles:
- `[TASK-XXX] Brief description`
- Examples:
  - `[TASK-001] Fix constructor argument order in source generator`
  - `[TASK-004] Add comprehensive EquatableArray tests`
  - `[TASK-008] Create getting started documentation`

### 6. PR Description Template

```markdown
## Summary
Brief description of the changes and their purpose.

## Related Issue
Closes #XXX

## Changes
- Bullet points of specific changes made
- Be specific about what was modified
- Include any breaking changes

## Testing
- Description of tests added/modified
- Current test coverage percentage
- Any manual testing performed

## Checklist
- [ ] All tests pass
- [ ] Code coverage ≥ 80%
- [ ] No compiler warnings
- [ ] Follows project conventions
- [ ] Updated documentation (if needed)
```

### 7. After Creating PR

1. **Check CI Status**: Ensure all checks pass
2. **Address Feedback**: If reviews are provided, address them promptly
3. **Keep PR Updated**: If main changes, rebase your branch:
   ```bash
   git checkout main
   git pull
   git checkout task-XXX-brief-description
   git rebase main
   git push --force-with-lease
   ```

### 8. Merging

- PRs should be merged via GitHub UI
- Use "Squash and merge" for clean history
- Delete branch after merge

## Common Scenarios

### Scenario: Multiple Commits for One Logical Change

```bash
# Interactive rebase to clean up history before PR
git rebase -i HEAD~3
# Mark commits to squash
# Force push
git push --force-with-lease
```

### Scenario: PR Feedback Requires Changes

```bash
# Make requested changes
git add .
git commit -m "fix: address PR feedback"
git push

# Or amend if small change
git add .
git commit --amend --no-edit
git push --force-with-lease
```

### Scenario: Conflicts with Main

```bash
# Update main
git checkout main
git pull

# Rebase your branch
git checkout task-XXX-brief-description
git rebase main

# Resolve conflicts, then
git add .
git rebase --continue
git push --force-with-lease
```

## Best Practices

1. **One Issue = One PR**: Each PR should address exactly one issue
2. **Small PRs**: Easier to review and less likely to have conflicts
3. **Descriptive Commits**: Use conventional commit format
4. **Test Everything**: Never submit untested code
5. **Document Changes**: Update docs if APIs change
6. **Link Issues**: Always use "Closes #XXX" to auto-close issues

## Commit Message Format

Follow conventional commits:

```
type: description

[optional body]

[optional footer(s)]
```

Types:
- `fix:` - Bug fixes
- `feat:` - New features
- `test:` - Test additions/changes
- `docs:` - Documentation only
- `style:` - Code style changes (formatting)
- `refactor:` - Code changes that neither fix bugs nor add features
- `perf:` - Performance improvements
- `chore:` - Maintenance tasks

Examples:
```
fix: correct constructor argument order in source generator

The EndpointAttribute constructor takes (method, pattern) but the
generator was extracting them in reverse order.
```

```
test: add unit tests for EquatableArray equality operations
```

```
docs: add getting started guide for new users
```

## GitHub CLI Quick Reference

```bash
# Create PR
gh pr create

# List PRs
gh pr list

# Check PR status
gh pr status

# View PR in browser
gh pr view --web

# Check out someone else's PR locally
gh pr checkout 123
```

## Integration with Issue Workflow

1. Agent picks issue from GitHub
2. Creates feature branch following naming convention
3. Implements solution with tests
4. Verifies completion criteria
5. Creates PR linking to issue
6. PR review and merge closes issue automatically

This workflow ensures:
- Clean git history
- Traceability (issues → PRs → commits)
- Quality control through reviews
- Automated issue management

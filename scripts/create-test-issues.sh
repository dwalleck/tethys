#!/bin/bash
# Create a few test issues to verify label format

cd github-issues

echo "Creating test issues..."

# TASK-000
echo "Creating TASK-000: Remove FluentAssertions..."
gh issue create \
    --title "[TASK-000] Remove FluentAssertions Due to Licensing" \
    --body-file "task-000.md" \
    --label "type: bug,priority: p0,phase: 0,size: small"

# TASK-001
echo "Creating TASK-001: Fix Constructor Argument Order..."
gh issue create \
    --title "[TASK-001] Fix Constructor Argument Order" \
    --body-file "task-001.md" \
    --label "type: bug,priority: p0,phase: 0,size: small"

# TASK-002
echo "Creating TASK-002: Fix Namespace Inconsistencies..."
gh issue create \
    --title "[TASK-002] Fix Namespace Inconsistencies" \
    --body-file "task-002.md" \
    --label "type: bug,priority: p0,phase: 0,size: small"

echo "Done! Check https://github.com/dwalleck/tethys/issues"
#!/bin/bash
# Create all GitHub issues for Tethys development plan

cd github-issues

echo "Creating all GitHub issues for Tethys..."
echo ""

# Check if issues already exist to avoid duplicates
existing_issues=$(gh issue list --limit 100 --json number,title | jq -r '.[].title')

create_issue_if_not_exists() {
    local task_id=$1
    local title=$2
    local labels=$3
    local file=$4
    
    # Check if issue already exists
    if echo "$existing_issues" | grep -q "\[$task_id\]"; then
        echo "Issue $task_id already exists, skipping..."
    else
        echo "Creating $task_id: ${title}..."
        gh issue create \
            --title "[$task_id] $title" \
            --body-file "$file" \
            --label "$labels"
    fi
}

# Phase 0: Critical Fixes
create_issue_if_not_exists "TASK-000" "Remove FluentAssertions Due to Licensing" "type: bug,priority: p0,phase: 0,size: small" "task-000.md"
create_issue_if_not_exists "TASK-001" "Fix Constructor Argument Order" "type: bug,priority: p0,phase: 0,size: small" "task-001.md"
create_issue_if_not_exists "TASK-002" "Fix Namespace Inconsistencies" "type: bug,priority: p0,phase: 0,size: small" "task-002.md"
create_issue_if_not_exists "TASK-003" "Clean Up Duplicate Test Projects" "type: testing,priority: p0,phase: 0,size: small" "task-003.md"

# Phase 1: Core Testing
create_issue_if_not_exists "TASK-004" "Comprehensive EquatableArray Tests" "type: testing,priority: p1,phase: 1,size: small" "task-004.md"
create_issue_if_not_exists "TASK-005" "Model Record Equality Tests" "type: testing,priority: p1,phase: 1,size: small" "task-005.md"
create_issue_if_not_exists "TASK-006" "Generator Logic Unit Tests" "type: testing,priority: p1,phase: 1,size: small" "task-006.md"
create_issue_if_not_exists "TASK-007" "Base Library Tests" "type: testing,priority: p1,phase: 1,size: small" "task-007.md"

# Phase 2: Documentation
create_issue_if_not_exists "TASK-008" "Getting Started Guide" "type: documentation,priority: p1,phase: 2,size: medium" "task-008.md"
create_issue_if_not_exists "TASK-009" "API Reference" "type: documentation,priority: p1,phase: 2,size: large" "task-009.md"
create_issue_if_not_exists "TASK-010" "Migration Guide" "type: documentation,priority: p2,phase: 2,size: medium" "task-010.md"
create_issue_if_not_exists "TASK-011" "Example Projects" "type: feature,priority: p2,phase: 2,size: large" "task-011.md"

# Phase 3: Package & Release
create_issue_if_not_exists "TASK-012" "NuGet Package Configuration" "type: infrastructure,priority: p1,phase: 3,size: small" "task-012.md"
create_issue_if_not_exists "TASK-013" "CI/CD Pipeline" "type: infrastructure,priority: p1,phase: 3,size: medium" "task-013.md"
create_issue_if_not_exists "TASK-014" "Initial Release Process" "type: infrastructure,priority: p1,phase: 3,size: small" "task-014.md"

# Phase 4: Quality & Performance
create_issue_if_not_exists "TASK-015" "Cacheability Tests" "type: testing,priority: p2,phase: 4,size: medium" "task-015.md"
create_issue_if_not_exists "TASK-016" "Performance Benchmarks" "type: performance,priority: p2,phase: 4,size: medium" "task-016.md"
create_issue_if_not_exists "TASK-017" "Expanded Integration Tests" "type: testing,priority: p2,phase: 4,size: large" "task-017.md"
create_issue_if_not_exists "TASK-018" "Comprehensive Snapshot Tests" "type: testing,priority: p2,phase: 4,size: medium" "task-018.md"

# Phase 5: Advanced Features
create_issue_if_not_exists "TASK-019" "Route Constraints Support" "type: feature,priority: p3,phase: 5,size: large" "task-019.md"
create_issue_if_not_exists "TASK-020" "Versioning Support" "type: feature,priority: p3,phase: 5,size: large" "task-020.md"
create_issue_if_not_exists "TASK-021" "Rate Limiting Integration" "type: feature,priority: p3,phase: 5,size: large" "task-021.md"
create_issue_if_not_exists "TASK-022" "Auth/AuthZ Helpers" "type: feature,priority: p3,phase: 5,size: large" "task-022.md"

echo ""
echo "All issues created! Check https://github.com/dwalleck/tethys/issues"
echo ""
echo "Summary:"
gh issue list --limit 50 --json number,title,labels | jq -r '.[] | "Issue #\(.number): \(.title)"' | sort -V
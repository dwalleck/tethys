#!/bin/bash
# Create all Stratify development tasks as GitHub issues

echo "Creating GitHub issues for Stratify development plan..."
echo ""

# Check if gh is installed
if ! command -v gh &> /dev/null; then
    echo "GitHub CLI (gh) is not installed. Please install it first."
    echo "Visit: https://cli.github.com/"
    exit 1
fi

# Check if authenticated
if ! gh auth status &> /dev/null; then
    echo "Not authenticated. Please run: gh auth login"
    exit 1
fi

# Create labels first
echo "Creating labels..."
gh label create "type: testing" --color "0366d6" --description "Testing tasks" 2>/dev/null
gh label create "type: bug" --color "d73a4a" --description "Bug fixes" 2>/dev/null
gh label create "type: feature" --color "0e8a16" --description "New features" 2>/dev/null
gh label create "type: documentation" --color "d4c5f9" --description "Documentation" 2>/dev/null
gh label create "type: performance" --color "fbca04" --description "Performance improvements" 2>/dev/null
gh label create "type: infrastructure" --color "1d76db" --description "Build/CI/Release" 2>/dev/null

gh label create "priority: p0" --color "b60205" --description "Critical priority" 2>/dev/null
gh label create "priority: p1" --color "d93f0b" --description "High priority" 2>/dev/null
gh label create "priority: p2" --color "fbca04" --description "Medium priority" 2>/dev/null
gh label create "priority: p3" --color "c2e0c6" --description "Low priority" 2>/dev/null

gh label create "phase: 0" --color "5319e7" --description "Critical fixes" 2>/dev/null
gh label create "phase: 1" --color "5319e7" --description "Core testing" 2>/dev/null
gh label create "phase: 2" --color "5319e7" --description "Documentation" 2>/dev/null
gh label create "phase: 3" --color "5319e7" --description "Package & Release" 2>/dev/null
gh label create "phase: 4" --color "5319e7" --description "Quality & Performance" 2>/dev/null
gh label create "phase: 5" --color "5319e7" --description "Advanced Features" 2>/dev/null

gh label create "size: small" --color "69db7c" --description "2-4 hours" 2>/dev/null
gh label create "size: medium" --color "f9d0c4" --description "1-2 days" 2>/dev/null
gh label create "size: large" --color "e99695" --description "3+ days" 2>/dev/null

echo ""
echo "Creating issues..."


# TASK-000
echo "Creating TASK-000: Remove FluentAssertions Due to Licensing..."
gh issue create \
    --title "[TASK-000] Remove FluentAssertions Due to Licensing" \
    --body-file "task-000.md" \
    --label "type: feature,priority: p0,phase: 0,size: small"

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

# TASK-003
echo "Creating TASK-003: Clean Up Duplicate Test Projects..."
gh issue create \
    --title "[TASK-003] Clean Up Duplicate Test Projects" \
    --body-file "task-003.md" \
    --label "type: testing,priority: p0,phase: 0,size: small"

# TASK-004
echo "Creating TASK-004: Comprehensive EquatableArray Tests..."
gh issue create \
    --title "[TASK-004] Comprehensive EquatableArray Tests" \
    --body-file "task-004.md" \
    --label "type: testing,priority: p1 (High),phase: 0,size: small"

# TASK-005
echo "Creating TASK-005: Model Record Equality Tests..."
gh issue create \
    --title "[TASK-005] Model Record Equality Tests" \
    --body-file "task-005.md" \
    --label "type: testing,priority: p1 (High),phase: 0,size: small"

# TASK-006
echo "Creating TASK-006: Generator Logic Unit Tests..."
gh issue create \
    --title "[TASK-006] Generator Logic Unit Tests" \
    --body-file "task-006.md" \
    --label "type: testing,priority: p1 (High),phase: 0,size: small"

# TASK-007
echo "Creating TASK-007: Base Library Tests..."
gh issue create \
    --title "[TASK-007] Base Library Tests" \
    --body-file "task-007.md" \
    --label "type: testing,priority: p1 (High),phase: 0,size: small"

# TASK-008
echo "Creating TASK-008: Getting Started Guide..."
gh issue create \
    --title "[TASK-008] Getting Started Guide" \
    --body-file "task-008.md" \
    --label "type: documentation,priority: p1 (High),phase: 0,size: medium"

# TASK-009
echo "Creating TASK-009: API Reference..."
gh issue create \
    --title "[TASK-009] API Reference" \
    --body-file "task-009.md" \
    --label "type: feature,priority: p1 (High),phase: 0,size: large"

# TASK-010
echo "Creating TASK-010: Migration Guide..."
gh issue create \
    --title "[TASK-010] Migration Guide" \
    --body-file "task-010.md" \
    --label "type: documentation,priority: p2 (Medium),phase: 0,size: medium"

# TASK-011
echo "Creating TASK-011: Example Projects..."
gh issue create \
    --title "[TASK-011] Example Projects" \
    --body-file "task-011.md" \
    --label "type: feature,priority: p2 (Medium),phase: 0,size: large"

# TASK-012
echo "Creating TASK-012: NuGet Package Configuration..."
gh issue create \
    --title "[TASK-012] NuGet Package Configuration" \
    --body-file "task-012.md" \
    --label "type: infrastructure,priority: p1 (High),phase: 0,size: small"

# TASK-013
echo "Creating TASK-013: CI/CD Pipeline..."
gh issue create \
    --title "[TASK-013] CI/CD Pipeline" \
    --body-file "task-013.md" \
    --label "type: feature,priority: p1 (High),phase: 0,size: medium"

# TASK-014
echo "Creating TASK-014: Initial Release Process..."
gh issue create \
    --title "[TASK-014] Initial Release Process" \
    --body-file "task-014.md" \
    --label "type: infrastructure,priority: p1 (High),phase: 0,size: small"

# TASK-015
echo "Creating TASK-015: Cacheability Tests..."
gh issue create \
    --title "[TASK-015] Cacheability Tests" \
    --body-file "task-015.md" \
    --label "type: testing,priority: p2 (Medium),phase: 0,size: medium"

# TASK-016
echo "Creating TASK-016: Performance Benchmarks..."
gh issue create \
    --title "[TASK-016] Performance Benchmarks" \
    --body-file "task-016.md" \
    --label "type: performance,priority: p2 (Medium),phase: 0,size: medium"

# TASK-017
echo "Creating TASK-017: Expanded Integration Tests..."
gh issue create \
    --title "[TASK-017] Expanded Integration Tests" \
    --body-file "task-017.md" \
    --label "type: testing,priority: p2 (Medium),phase: 0,size: large"

# TASK-018
echo "Creating TASK-018: Comprehensive Snapshot Tests..."
gh issue create \
    --title "[TASK-018] Comprehensive Snapshot Tests" \
    --body-file "task-018.md" \
    --label "type: testing,priority: p2 (Medium),phase: 0,size: medium"

# TASK-019
echo "Creating TASK-019: Route Constraints Support..."
gh issue create \
    --title "[TASK-019] Route Constraints Support" \
    --body-file "task-019.md" \
    --label "type: feature,priority: p3 (Low),phase: 0,size: large"

# TASK-020
echo "Creating TASK-020: Versioning Support..."
gh issue create \
    --title "[TASK-020] Versioning Support" \
    --body-file "task-020.md" \
    --label "type: feature,priority: p3 (Low),phase: 0,size: large"

# TASK-021
echo "Creating TASK-021: Rate Limiting Integration..."
gh issue create \
    --title "[TASK-021] Rate Limiting Integration" \
    --body-file "task-021.md" \
    --label "type: feature,priority: p3 (Low),phase: 0,size: large"

# TASK-022
echo "Creating TASK-022: Auth/AuthZ Helpers..."
gh issue create \
    --title "[TASK-022] Auth/AuthZ Helpers" \
    --body-file "task-022.md" \
    --label "type: feature,priority: p3 (Low),phase: 0,size: large"

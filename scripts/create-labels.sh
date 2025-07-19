#!/bin/bash
# Create all GitHub labels for Tethys

echo "Creating GitHub labels for Tethys repository..."
echo ""

# Type labels
echo "Creating type labels..."
gh label create "type: testing" --color "0366d6" --description "Testing tasks" 2>/dev/null || echo "  type: testing already exists"
gh label create "type: bug" --color "d73a4a" --description "Bug fixes" 2>/dev/null || echo "  type: bug already exists"
gh label create "type: feature" --color "0e8a16" --description "New features" 2>/dev/null || echo "  type: feature already exists"
gh label create "type: documentation" --color "d4c5f9" --description "Documentation" 2>/dev/null || echo "  type: documentation already exists"
gh label create "type: performance" --color "fbca04" --description "Performance improvements" 2>/dev/null || echo "  type: performance already exists"
gh label create "type: infrastructure" --color "1d76db" --description "Build/CI/Release" 2>/dev/null || echo "  type: infrastructure already exists"

# Priority labels
echo ""
echo "Creating priority labels..."
gh label create "priority: p0" --color "b60205" --description "Critical priority" 2>/dev/null || echo "  priority: p0 already exists"
gh label create "priority: p1" --color "d93f0b" --description "High priority" 2>/dev/null || echo "  priority: p1 already exists"
gh label create "priority: p2" --color "fbca04" --description "Medium priority" 2>/dev/null || echo "  priority: p2 already exists"
gh label create "priority: p3" --color "c2e0c6" --description "Low priority" 2>/dev/null || echo "  priority: p3 already exists"

# Phase labels
echo ""
echo "Creating phase labels..."
gh label create "phase: 0" --color "5319e7" --description "Critical fixes" 2>/dev/null || echo "  phase: 0 already exists"
gh label create "phase: 1" --color "5319e7" --description "Core testing" 2>/dev/null || echo "  phase: 1 already exists"
gh label create "phase: 2" --color "5319e7" --description "Documentation" 2>/dev/null || echo "  phase: 2 already exists"
gh label create "phase: 3" --color "5319e7" --description "Package & Release" 2>/dev/null || echo "  phase: 3 already exists"
gh label create "phase: 4" --color "5319e7" --description "Quality & Performance" 2>/dev/null || echo "  phase: 4 already exists"
gh label create "phase: 5" --color "5319e7" --description "Advanced Features" 2>/dev/null || echo "  phase: 5 already exists"

# Size labels
echo ""
echo "Creating size labels..."
gh label create "size: small" --color "69db7c" --description "2-4 hours" 2>/dev/null || echo "  size: small already exists"
gh label create "size: medium" --color "f9d0c4" --description "1-2 days" 2>/dev/null || echo "  size: medium already exists"
gh label create "size: large" --color "e99695" --description "3+ days" 2>/dev/null || echo "  size: large already exists"

echo ""
echo "All labels created successfully!"
echo ""
echo "To see all labels: gh label list"
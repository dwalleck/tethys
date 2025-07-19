#!/bin/bash
# Create GitHub milestones for Tethys

echo "Creating GitHub milestones..."
echo ""

# Create milestones with due dates
gh api repos/dwalleck/tethys/milestones \
  --method POST \
  -f title="Phase 0: Critical Fixes" \
  -f description="Fix blocking issues preventing framework usage" \
  -f due_on="2025-02-01T00:00:00Z" \
  -f state="open" 2>/dev/null || echo "Phase 0 milestone might already exist"

gh api repos/dwalleck/tethys/milestones \
  --method POST \
  -f title="Phase 1: Core Testing" \
  -f description="Implement fundamental test coverage" \
  -f due_on="2025-02-15T00:00:00Z" \
  -f state="open" 2>/dev/null || echo "Phase 1 milestone might already exist"

gh api repos/dwalleck/tethys/milestones \
  --method POST \
  -f title="Phase 2: Documentation" \
  -f description="Create comprehensive documentation" \
  -f due_on="2025-03-01T00:00:00Z" \
  -f state="open" 2>/dev/null || echo "Phase 2 milestone might already exist"

gh api repos/dwalleck/tethys/milestones \
  --method POST \
  -f title="Phase 3: Package & Release" \
  -f description="Package and publish to NuGet" \
  -f due_on="2025-03-15T00:00:00Z" \
  -f state="open" 2>/dev/null || echo "Phase 3 milestone might already exist"

gh api repos/dwalleck/tethys/milestones \
  --method POST \
  -f title="Phase 4: Quality & Performance" \
  -f description="Advanced testing and performance optimization" \
  -f due_on="2025-04-01T00:00:00Z" \
  -f state="open" 2>/dev/null || echo "Phase 4 milestone might already exist"

gh api repos/dwalleck/tethys/milestones \
  --method POST \
  -f title="Phase 5: Advanced Features" \
  -f description="Additional features and enhancements" \
  -f due_on="2025-05-01T00:00:00Z" \
  -f state="open" 2>/dev/null || echo "Phase 5 milestone might already exist"

echo ""
echo "Milestones created!"
echo ""
echo "Current milestones:"
gh api repos/dwalleck/tethys/milestones --jq '.[] | "- \(.title) (due: \(.due_on // "no due date"))"'
#!/bin/bash
# Task verification script for Tethys development
# Usage: ./scripts/verify-task.sh TASK-XXX

set -e

TASK_ID=$1

if [ -z "$TASK_ID" ]; then
    echo "Usage: ./scripts/verify-task.sh TASK-XXX"
    exit 1
fi

echo "=== Verifying $TASK_ID ==="

# Find task details from github issues
TASK_FILE=$(find github-issues -name "${TASK_ID,,}-*.md" 2>/dev/null | head -1)

if [ -z "$TASK_FILE" ]; then
    echo "❌ Task $TASK_ID not found in github-issues/"
    exit 1
fi

echo "✓ Found task file: $TASK_FILE"

# Check if code compiles
echo -n "Checking if solution builds... "
if dotnet build --nologo --verbosity quiet; then
    echo "✓"
else
    echo "❌"
    echo "Build failed! Fix compilation errors before marking task complete."
    exit 1
fi

# Run tests
echo -n "Running tests... "
if dotnet test --nologo --verbosity quiet; then
    echo "✓"
else
    echo "❌"
    echo "Tests failed! All tests must pass."
    exit 1
fi

# Check code coverage for test projects
echo "Checking code coverage..."
COVERAGE_PROJECTS=(
    "test/Tethys.MinimalEndpoints.ImprovedSourceGenerators.Tests"
    "test/Tethys.ImprovedSourceGenerators.SnapshotTests"
    "test/Tethys.ImprovedSourceGenerators.IntegrationTests"
)

for PROJECT in "${COVERAGE_PROJECTS[@]}"; do
    if [ -d "$PROJECT" ]; then
        echo -n "  $PROJECT: "
        COVERAGE_OUTPUT=$(dotnet test "$PROJECT" /p:CollectCoverage=true /p:CoverletOutputFormat=json /p:Threshold=80 /p:ThresholdType=line --nologo --verbosity quiet 2>&1 || true)
        
        if echo "$COVERAGE_OUTPUT" | grep -q "The total line coverage is below the specified"; then
            echo "❌ Below 80%"
            echo "    Run this for details: dotnet test $PROJECT /p:CollectCoverage=true"
        else
            # Try to extract coverage percentage
            COVERAGE=$(echo "$COVERAGE_OUTPUT" | grep -oP 'Total.*?(\d+\.\d+)%' | grep -oP '\d+\.\d+' | head -1)
            if [ -n "$COVERAGE" ]; then
                echo "✓ ($COVERAGE%)"
            else
                echo "✓"
            fi
        fi
    fi
done

# Task-specific checks based on task number
case $TASK_ID in
    "TASK-001")
        # Check for fixed constructor argument
        if grep -q "Method = method;" src/Tethys.MinimalEndpoints.ImprovedSourceGenerators/EndpointGeneratorImproved.cs; then
            echo "✓ Constructor argument order appears fixed"
        else
            echo "⚠️  Verify constructor argument order is fixed"
        fi
        ;;
    
    "TASK-002")
        # Check snapshot tests removed
        if [ -f "test/Tethys.MinimalEndpoints.ImprovedSourceGenerators.Tests/*Snapshot*.cs" ]; then
            echo "❌ Snapshot tests still exist in main test project"
            exit 1
        else
            echo "✓ Snapshot tests removed from main project"
        fi
        ;;
    
    "TASK-003")
        # Check test helpers updated
        if grep -q "Tethys.MinimalEndpoints.Attributes" test/*/TestHelper.cs 2>/dev/null; then
            echo "✓ Test helpers use correct namespace"
        else
            echo "⚠️  Verify test helpers use correct attribute namespace"
        fi
        ;;
    
    "TASK-004")
        # Check EquatableArray tests
        if [ -f "test/Tethys.MinimalEndpoints.ImprovedSourceGenerators.Tests/EquatableArrayTests.cs" ]; then
            echo "✓ EquatableArray tests exist"
        else
            echo "❌ EquatableArray tests not found"
        fi
        ;;
    
    *)
        echo "⚠️  No specific verification rules for $TASK_ID"
        ;;
esac

echo ""
echo "=== Acceptance Criteria Checklist ==="
echo "Please manually verify these items from $TASK_FILE:"
echo ""

# Extract and display acceptance criteria
if [ -f "$TASK_FILE" ]; then
    CRITERIA=$(grep -A20 "Acceptance Criteria" "$TASK_FILE" | grep -E "^- \[" || echo "")
    if [ -n "$CRITERIA" ]; then
        echo "$CRITERIA"
    else
        echo "No acceptance criteria found in task description"
    fi
fi

echo ""
echo "=== Verification Summary ==="
echo "✓ Code compiles"
echo "✓ Tests pass"

# Check if session notes were updated today
echo ""
echo -n "Checking session notes... "
if [ -f "SESSION_NOTES.md" ]; then
    LAST_UPDATE=$(grep "## Session:" SESSION_NOTES.md 2>/dev/null | tail -1 | grep -o "[0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\}" || echo "")
    TODAY=$(date +%Y-%m-%d)
    if [ "$LAST_UPDATE" = "$TODAY" ]; then
        echo "✓ (Updated today)"
    else
        echo "⚠️  (Last updated: ${LAST_UPDATE:-never})"
        echo "   Remember to update SESSION_NOTES.md!"
    fi
else
    echo "❌ (SESSION_NOTES.md not found)"
fi

echo ""
echo "⚠️  Manual verification required for:"
echo "- All acceptance criteria above"
echo "- Code follows project patterns"
echo "- Documentation is updated"
echo "- Coverage is ≥80% for new code"
echo "- Session notes updated"
echo ""
echo "If all checks pass, you can mark $TASK_ID as complete!"
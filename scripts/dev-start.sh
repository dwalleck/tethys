#!/bin/bash
# Development session startup script for Tethys

echo "🚀 Starting Tethys Development Session"
echo "====================================="
echo ""

# Update from main
echo "📥 Pulling latest changes..."
git pull origin main --quiet || echo "⚠️  Could not pull from main"

# Show current branch
BRANCH=$(git branch --show-current)
echo "📍 Current branch: $BRANCH"

# Check for uncommitted changes
if ! git diff-index --quiet HEAD --; then
    echo "⚠️  You have uncommitted changes:"
    git status --short
    echo ""
fi

# Run task status
echo ""
echo "📊 Task Status:"
python3 scripts/task-status.py --next

# Check session notes
echo ""
echo "📝 Last session notes:"
if [ -f "SESSION_NOTES.md" ]; then
    tail -n 20 SESSION_NOTES.md | grep -A20 "## Session:" | tail -n 15
else
    echo "No session notes found. Creating SESSION_NOTES.md..."
    cat > SESSION_NOTES.md << EOF
# Tethys Development Session Notes

Track your daily progress here. Each session should have:
- What you completed
- What you're working on
- Any blockers
- Time spent vs estimated

## Session: $(date '+%Y-%m-%d %H:%M')
### Completed
- Initial setup

### In Progress
- Review available tasks

### Blockers
- None

### Time Spent
- Estimated: N/A
- Actual: 5 minutes
EOF
fi

# Build check
echo ""
echo "🔨 Checking build..."
if dotnet build --nologo --verbosity quiet; then
    echo "✅ Build successful"
else
    echo "❌ Build failed - fix before starting work"
fi

# Test check
echo ""
echo "🧪 Running quick test check..."
TEST_COUNT=$(dotnet test --nologo --verbosity quiet --no-build 2>&1 | grep -oP 'Total:\s*\K\d+' | head -1 || echo "0")
if [ -n "$TEST_COUNT" ] && [ "$TEST_COUNT" -gt "0" ]; then
    echo "✅ Found $TEST_COUNT tests"
else
    echo "⚠️  Could not determine test count"
fi

# Daily checklist reminder
echo ""
echo "📋 Daily Checklist:"
echo "  □ Review task requirements"
echo "  □ Create feature branch"
echo "  □ Write tests first (TDD)"
echo "  □ Commit frequently"
echo "  □ Update session notes"

# Show available commands
echo ""
echo "🛠️  Useful Commands:"
echo "  ./scripts/task-status.py      - Show all tasks"
echo "  ./scripts/verify-task.sh TASK-XXX - Verify task completion"
echo "  ./scripts/coverage-report.sh  - Generate coverage report"
echo "  dotnet test                   - Run all tests"
echo ""

echo "Ready to start! Pick a task with: ./scripts/task-status.py"
echo ""
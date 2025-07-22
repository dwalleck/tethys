#!/usr/bin/env python3
"""
Export Stratify test tasks to GitHub issues format.
This script parses TEST_IMPLEMENTATION_PLAN.md and creates individual issue files.
"""

import re
import os
from pathlib import Path

def parse_test_plan(file_path):
    """Parse the test implementation plan and extract tasks."""
    with open(file_path, 'r') as f:
        content = f.read()

    tasks = []
    task_id = 0

    # Parse phases
    phase_pattern = r'### Phase (\d+):[^#]+?(.*?)(?=###|\Z)'

    for phase_match in re.finditer(phase_pattern, content, re.DOTALL):
        phase_num = phase_match.group(1)
        phase_content = phase_match.group(2)

        # Extract phase details
        priority_match = re.search(r'\*\*Priority\*\*:\s*(.+)', phase_content)
        priority = priority_match.group(1) if priority_match else 'CRITICAL' if phase_num == '0' else 'HIGH'

        # Extract time estimate
        time_match = re.search(r'\((\d+[-\d]*)\s*(days?|hours?)\)', phase_content)
        estimated = time_match.group(0).strip('()') if time_match else '2-4 hours'

        # Extract tasks from checkbox items
        task_pattern = r'- \[ \] (.+?)(?:\n|$)'
        for task_match in re.finditer(task_pattern, phase_content):
            task_id += 1
            task_desc = task_match.group(1).strip()

            # Determine task type
            if 'test' in task_desc.lower():
                task_type = 'Testing'
            elif 'fix' in task_desc.lower() or 'update' in task_desc.lower():
                task_type = 'Bug Fix'
            elif 'document' in task_desc.lower():
                task_type = 'Documentation'
            else:
                task_type = 'Feature'

            # Map phase to milestone
            milestone_map = {
                '0': 'Critical Fixes',
                '1': 'Core Testing',
                '2': 'Cacheability Testing',
                '3': 'Snapshot Testing',
                '4': 'Performance Testing',
                '5': 'Integration Testing',
                '6': 'Package Testing'
            }

            task = {
                'id': f'TASK-{task_id:03d}',
                'title': task_desc,
                'type': task_type,
                'priority': 'P0' if phase_num == '0' else ('P1' if phase_num in ['1', '2'] else 'P2'),
                'milestone': milestone_map.get(phase_num, 'Backlog'),
                'phase': f'Phase {phase_num}',
                'estimated': estimated,
                'body': generate_task_body(task_desc, phase_num, phase_content)
            }

            tasks.append(task)

    return tasks

def generate_task_body(task_title, phase_num, phase_content):
    """Generate detailed task body with implementation guidance."""

    body = f"""**Type**: Testing
**Phase**: Phase {phase_num}
**Component**: Stratify.MinimalEndpoints

**Description**: {task_title}

**Technical Details**:
"""

    # Add specific implementation guidance based on task
    if 'EquatableArray' in task_title:
        body += """
- Test all operations: equality, GetHashCode, operators
- Test with different types (value types, reference types)
- Test edge cases: null elements, empty arrays
- Verify immutability behavior

**Files to Modify**:
- `test/Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests/EquatableArrayTests.cs` (create)

**Test Pattern**:
```csharp
[Test]
public async Task Equality_WithSameElements_ReturnsTrue()
{
    var array1 = new EquatableArray<string>(["a", "b"]);
    var array2 = new EquatableArray<string>(["a", "b"]);

    await Assert.That(array1).IsEqualTo(array2);
    await Assert.That(array1 == array2).IsTrue();
}
```
"""

    elif 'model record equality' in task_title.lower():
        body += """
- Test equality for all model records
- Test with different property values
- Verify GetHashCode distribution
- Test null handling

**Models to Test**:
- EndpointClass
- EndpointMetadata
- HandlerMethod
- MethodParameter

**Files to Modify**:
- `test/Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests/ModelEqualityTests.cs` (create)
"""

    elif 'constructor argument order' in task_title.lower():
        body += """
- Fix the ExtractHttpMethod to use correct index (0)
- Fix the ExtractPattern to use correct index (1)
- Update tests to verify correct extraction

**Files to Modify**:
- `src/Stratify.MinimalEndpoints.ImprovedSourceGenerators/EndpointGeneratorImproved.cs`
- Related test files

**Current Issue**:
Constructor: `EndpointAttribute(HttpMethodType method, string pattern)`
But extraction uses wrong indices.
"""

    elif 'cacheability' in task_title.lower():
        body += """
- Implement tests following Andrew Lock's Part 10 guide
- Test that unchanged input produces cached output
- Test that whitespace changes don't break cache
- Verify incremental compilation performance

**Files to Create**:
- `test/Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests/CacheabilityTests.cs`

**Reference**: SOURCE_GENERATOR.md Section on Cacheability Testing
"""

    elif 'snapshot' in task_title.lower():
        body += """
- Use Verify.TUnit for snapshot testing
- Test various endpoint configurations
- Test edge cases and complex scenarios
- Review all generated output

**Files to Modify**:
- `test/Stratify.ImprovedSourceGenerators.SnapshotTests/`

**Pattern**: See TestHelper.cs for verification setup
"""

    # Add acceptance criteria
    body += """

**Acceptance Criteria**:
- [ ] Tests are written using TUnit (not xUnit)
- [ ] All tests pass
- [ ] Code coverage ≥ 80% for affected code
- [ ] Follows existing test patterns
- [ ] No compiler warnings
"""

    return body

def create_github_issue(task):
    """Create GitHub issue content from task."""
    labels = []

    # Add type label
    labels.append(f"type: {task['type'].lower().replace(' ', '-')}")

    # Add priority label
    labels.append(f"priority: {task['priority'].lower()}")

    # Add milestone as label
    milestone_label = task['milestone'].lower().replace(' ', '-')
    labels.append(f"milestone: {milestone_label}")

    # Add phase label
    phase_label = task['phase'].lower().replace(' ', '-')
    labels.append(f"phase: {phase_label}")

    # Add estimation label
    if 'hour' in task['estimated']:
        labels.append(f"size: small")
    elif 'day' in task['estimated']:
        labels.append(f"size: medium")
    else:
        labels.append(f"size: large")

    issue_content = f"""---
title: "[{task['id']}] {task['title']}"
labels: [{', '.join(f'"{label}"' for label in labels)}]
---

## Task: {task['id']} - {task['title']}

**Priority**: {task['priority']}
**Milestone**: {task['milestone']}
**Estimated**: {task['estimated']}

{task['body']}

---

### Dependencies
"""

    # Add dependency information
    task_num = int(task['id'].split('-')[1])
    if task_num > 1:
        if task['phase'] == 'Phase 0':
            issue_content += "None - Critical fix\n"
        else:
            issue_content += f"- Requires Phase 0 tasks to be completed\n"
            if task_num > 6:
                issue_content += f"- May depend on earlier tasks in {task['phase']}\n"
    else:
        issue_content += "None - Can start immediately\n"

    issue_content += """
---

_This issue was automatically generated from TEST_IMPLEMENTATION_PLAN.md_
"""

    return issue_content

def export_to_csv(tasks, output_file):
    """Export tasks to CSV format for GitHub bulk import."""
    import csv

    with open(output_file, 'w', newline='', encoding='utf-8') as f:
        writer = csv.writer(f)
        writer.writerow(['Title', 'Body', 'Labels', 'Milestone', 'Assignee'])

        for task in tasks:
            labels = []
            labels.append(f"type:{task['type'].lower().replace(' ', '-')}")
            labels.append(f"priority:{task['priority'].lower()}")
            labels.append(f"phase:{task['phase'].lower().replace(' ', '-')}")

            if 'hour' in task['estimated']:
                labels.append(f"size:small")
            elif 'day' in task['estimated']:
                labels.append(f"size:medium")
            else:
                labels.append(f"size:large")

            writer.writerow([
                f"[{task['id']}] {task['title']}",
                task['body'],
                ','.join(labels),
                task['milestone'],
                ''  # Assignee - leave empty
            ])

def main():
    """Main entry point"""
    # Look for test plan
    plan_path = Path(__file__).parent.parent / 'TEST_IMPLEMENTATION_PLAN.md'
    if not plan_path.exists():
        plan_path = Path(__file__).parent.parent / 'TEST-COVERAGE-PLAN.md'

    if not plan_path.exists():
        print("ERROR: No test plan found. Looking for TEST_IMPLEMENTATION_PLAN.md")
        return

    tasks = parse_test_plan(plan_path)

    print(f"Found {len(tasks)} tasks in test plan")

    # Create output directory
    output_dir = Path(__file__).parent.parent / 'github-issues'
    output_dir.mkdir(exist_ok=True)

    # Create individual issue files
    for task in tasks:
        issue_content = create_github_issue(task)
        # Clean the title for filename
        clean_title = re.sub(r'[^\w\s-]', '', task['title'].lower())
        clean_title = re.sub(r'[-\s]+', '-', clean_title)[:50]
        file_name = f"{task['id'].lower()}-{clean_title}.md"
        file_path = output_dir / file_name

        with open(file_path, 'w') as f:
            f.write(issue_content)

    print(f"Created {len(tasks)} individual issue files in {output_dir}")

    # Create CSV for bulk import
    csv_path = output_dir / 'github-issues-bulk.csv'
    export_to_csv(tasks, csv_path)
    print(f"Created bulk import CSV at {csv_path}")

    # Create import instructions
    create_import_instructions(output_dir)

    print("\nDone! Check the github-issues directory for all exported issues.")

def create_import_instructions(output_dir):
    """Create import instructions file"""
    instructions_path = output_dir / 'IMPORT-INSTRUCTIONS.md'

    with open(instructions_path, 'w') as f:
        f.write("""# GitHub Issues Import Instructions for Stratify

## Option 1: GitHub CLI (Recommended)

Install GitHub CLI and run:

```bash
cd github-issues
for file in task-*.md; do
  gh issue create --body-file "$file"
done
```

## Option 2: Bulk Import via CSV

1. Go to your GitHub repository
2. Use a CSV import tool or GitHub's bulk import feature
3. Upload `github-issues-bulk.csv`

## Option 3: Manual Creation

Each `.md` file in this directory represents one issue.
Copy the content and create issues manually.

## Labels to Create First

Before importing, create these labels in your repository:

### Type Labels
- `type: testing` (blue)
- `type: bug-fix` (red)
- `type: feature` (green)
- `type: documentation` (gray)

### Priority Labels
- `priority: p0` (red) - Critical, blocking other work
- `priority: p1` (orange) - High priority
- `priority: p2` (yellow) - Normal priority

### Phase Labels
- `phase: phase-0` - Critical fixes
- `phase: phase-1` - Core testing
- `phase: phase-2` - Cacheability
- `phase: phase-3` - Snapshots
- `phase: phase-4` - Performance
- `phase: phase-5` - Integration
- `phase: phase-6` - Package

### Size Labels
- `size: small` (2-4 hours)
- `size: medium` (1-2 days)
- `size: large` (3+ days)

### Milestone Labels
- `milestone: critical-fixes`
- `milestone: core-testing`
- `milestone: cacheability-testing`
- `milestone: snapshot-testing`
- `milestone: performance-testing`
- `milestone: integration-testing`
- `milestone: package-testing`

## GitHub Milestones

Create these milestones in your repository:

1. **Critical Fixes** - Fix blocking issues
2. **Core Testing** - Implement fundamental tests
3. **Advanced Testing** - Cacheability, performance, integration
4. **Package & Polish** - Final testing and packaging

## Import Order

1. Create all labels first
2. Create milestones
3. Import issues using one of the methods above
4. Assign issues to appropriate milestones

## Using MCP GitHub Tool

If using the GitHub MCP tool:

```bash
# Create labels
mcp github create-label "type: testing" --color "0366d6"
mcp github create-label "priority: p0" --color "d73a4a"
# ... etc

# Create issues
mcp github create-issue --title "[TASK-001] Fix constructor" --body-file task-001.md --labels "type:bug-fix,priority:p0"
```
""")

    print(f"Created import instructions at {instructions_path}")

if __name__ == "__main__":
    main()

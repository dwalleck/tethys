#!/usr/bin/env python3
"""
Export Stratify development plan tasks to GitHub issues format.
This script parses DEVELOPMENT-PLAN.md and creates individual issue files.
"""

import re
import os
from pathlib import Path
import json

def parse_development_plan(file_path):
    """Parse the development plan and extract tasks."""
    with open(file_path, 'r') as f:
        content = f.read()

    tasks = []
    current_phase = None

    # Parse tasks
    task_pattern = r'### (TASK-\d+): (.+?)\n(.*?)(?=###|\Z)'

    for match in re.finditer(task_pattern, content, re.DOTALL):
        task_id = match.group(1)
        title = match.group(2)
        task_content = match.group(3)

        # Extract task details
        priority = re.search(r'\*\*Priority\*\*:\s*([^\n]+)', task_content)
        estimated = re.search(r'\*\*Estimated\*\*:\s*([^\n]+)', task_content)
        dependencies = re.search(r'\*\*Dependencies\*\*:\s*([^\n]+)', task_content)
        blocks = re.search(r'\*\*Blocks\*\*:\s*([^\n]+)', task_content)
        description = re.search(r'\*\*Description\*\*:\s*([^\n]+)', task_content)

        # Extract success criteria
        criteria_match = re.search(r'\*\*Success Criteria\*\*:(.*?)(?=\*\*|$)', task_content, re.DOTALL)
        criteria = []
        if criteria_match:
            criteria_text = criteria_match.group(1)
            criteria = re.findall(r'- \[ \] (.+)', criteria_text)

        # Extract files to modify
        files_match = re.search(r'\*\*Files to Modify\*\*:(.*?)(?=###|$)', task_content, re.DOTALL)
        files = []
        if files_match:
            files_text = files_match.group(1)
            files = re.findall(r'- `(.+?)`', files_text)

        # Determine phase from section
        phase_match = re.search(r'## Phase (\d+):', content[:match.start()])
        if phase_match:
            current_phase = phase_match.group(1)

        task = {
            'id': task_id,
            'title': title,
            'priority': priority.group(1) if priority else 'P2',
            'estimated': estimated.group(1) if estimated else '1 day',
            'dependencies': dependencies.group(1) if dependencies else 'None',
            'blocks': blocks.group(1) if blocks else 'None',
            'description': description.group(1) if description else '',
            'success_criteria': criteria,
            'files': files,
            'phase': current_phase or '0'
        }

        tasks.append(task)

    return tasks

def determine_task_type(task):
    """Determine task type based on title and content."""
    title_lower = task['title'].lower()

    if 'test' in title_lower:
        return 'testing'
    elif 'fix' in title_lower or 'bug' in title_lower:
        return 'bug'
    elif 'document' in title_lower or 'guide' in title_lower:
        return 'documentation'
    elif 'benchmark' in title_lower or 'performance' in title_lower:
        return 'performance'
    elif 'release' in title_lower or 'package' in title_lower:
        return 'infrastructure'
    else:
        return 'feature'

def determine_size(estimated):
    """Determine size label based on time estimate."""
    if 'hour' in estimated:
        return 'small'
    elif '1 day' in estimated or '1-2 days' in estimated:
        return 'medium'
    else:
        return 'large'

def create_github_issue(task):
    """Create GitHub issue content from task."""
    task_type = determine_task_type(task)
    size = determine_size(task['estimated'])

    # Fix priority label format (P0 -> p0)
    priority = task['priority'].replace('P', 'p').replace(' (Critical - Legal)', '').replace(' (Critical)', '')

    labels = [
        f"type: {task_type}",
        f"priority: {priority}",
        f"phase: {task['phase']}",
        f"size: {size}"
    ]

    # Build issue body
    body = f"""## Task: {task['id']} - {task['title']}

**Type**: {task_type.title()}
**Priority**: {task['priority']}
**Estimated**: {task['estimated']}
**Phase**: {task['phase']}

### Description
{task['description']}

### Dependencies
{task['dependencies']}

### Blocks
{task['blocks']}
"""

    if task['success_criteria']:
        body += "\n### Success Criteria\n"
        for criteria in task['success_criteria']:
            body += f"- [ ] {criteria}\n"

    if task['files']:
        body += "\n### Files to Modify\n"
        for file in task['files']:
            body += f"- `{file}`\n"

    body += """
### Implementation Notes
- Use TUnit for all tests (not xUnit)
- Follow existing patterns in the codebase
- Ensure 80%+ code coverage for new code
- Update documentation if APIs change

### Verification
Run `./scripts/verify-task.sh {task_id}` to verify completion.

---
_This issue was automatically generated from DEVELOPMENT-PLAN.md_
""".format(task_id=task['id'])

    return {
        'title': f"[{task['id']}] {task['title']}",
        'body': body,
        'labels': labels
    }

def export_to_github_cli(tasks, output_dir):
    """Create a script to bulk create issues using GitHub CLI."""
    script_path = output_dir / 'create-all-issues.sh'

    with open(script_path, 'w') as f:
        f.write("""#!/bin/bash
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

""")

        for task in tasks:
            issue = create_github_issue(task)
            labels_str = ','.join(issue['labels'])

            # Escape quotes in title and body
            title = issue['title'].replace('"', '\\"')

            f.write(f"""
# {task['id']}
echo "Creating {task['id']}: {task['title']}..."
gh issue create \\
    --title "{title}" \\
    --body-file "{task['id'].lower()}.md" \\
    --label "{labels_str}"
""")

    os.chmod(script_path, 0o755)
    print(f"Created bulk creation script: {script_path}")

def create_summary_report(tasks, output_dir):
    """Create a summary report of all tasks."""
    summary_path = output_dir / 'TASK-SUMMARY.md'

    with open(summary_path, 'w') as f:
        f.write("# Stratify Development Tasks Summary\n\n")
        f.write(f"Total Tasks: {len(tasks)}\n\n")

        # Group by phase
        phases = {}
        for task in tasks:
            phase = f"Phase {task['phase']}"
            if phase not in phases:
                phases[phase] = []
            phases[phase].append(task)

        # Summary by phase
        f.write("## Tasks by Phase\n\n")
        for phase in sorted(phases.keys()):
            f.write(f"### {phase} ({len(phases[phase])} tasks)\n")
            for task in phases[phase]:
                deps = "None" if task['dependencies'] == "None" else f"Depends on: {task['dependencies']}"
                f.write(f"- **{task['id']}**: {task['title']} ({task['estimated']}) - {deps}\n")
            f.write("\n")

        # Priority summary
        f.write("## Priority Distribution\n\n")
        priorities = {}
        for task in tasks:
            p = task['priority']
            priorities[p] = priorities.get(p, 0) + 1

        for p in sorted(priorities.keys()):
            f.write(f"- {p}: {priorities[p]} tasks\n")

        # Time estimates
        f.write("\n## Time Estimates\n\n")
        total_hours = 0
        total_days = 0

        for task in tasks:
            est = task['estimated']
            if 'hour' in est:
                hours = re.search(r'(\d+)', est)
                if hours:
                    total_hours += int(hours.group(1))
            elif 'day' in est:
                days = re.search(r'(\d+)', est)
                if days:
                    total_days += int(days.group(1))

        f.write(f"- Total estimated time: {total_days} days + {total_hours} hours\n")
        f.write(f"- Approximate total: {total_days + total_hours/8:.1f} days\n")

def main():
    """Main entry point"""
    # Look for development plan
    plan_path = Path(__file__).parent.parent / 'DEVELOPMENT-PLAN.md'

    if not plan_path.exists():
        print("ERROR: DEVELOPMENT-PLAN.md not found")
        return

    tasks = parse_development_plan(plan_path)

    print(f"Found {len(tasks)} tasks in development plan")

    # Create output directory
    output_dir = Path(__file__).parent.parent / 'github-issues'
    output_dir.mkdir(exist_ok=True)

    # Create individual issue files
    for task in tasks:
        issue = create_github_issue(task)
        file_name = f"{task['id'].lower()}.md"
        file_path = output_dir / file_name

        with open(file_path, 'w') as f:
            # Write front matter for GitHub
            f.write(f"---\n")
            f.write(f"title: \"{issue['title']}\"\n")
            f.write(f"labels: [{', '.join(f'\"{l}\"' for l in issue['labels'])}]\n")
            f.write(f"---\n\n")
            f.write(issue['body'])

    print(f"Created {len(tasks)} individual issue files in {output_dir}")

    # Create bulk creation script
    export_to_github_cli(tasks, output_dir)

    # Create summary report
    create_summary_report(tasks, output_dir)

    print("\nDone! Check the github-issues directory for:")
    print("- Individual issue files (task-xxx.md)")
    print("- Bulk creation script (create-all-issues.sh)")
    print("- Task summary report (TASK-SUMMARY.md)")
    print("\nTo create all issues at once, run: ./github-issues/create-all-issues.sh")

if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""
Track task completion status and find next available tasks for Tethys.
"""

import re
import os
from pathlib import Path
from collections import defaultdict

def parse_tasks_from_plan():
    """Parse all tasks from TEST_IMPLEMENTATION_PLAN.md"""
    plan_path = Path(__file__).parent.parent / 'TEST_IMPLEMENTATION_PLAN.md'
    if not plan_path.exists():
        # Fallback to TEST-COVERAGE-PLAN.md if it exists
        plan_path = Path(__file__).parent.parent / 'TEST-COVERAGE-PLAN.md'
    
    if not plan_path.exists():
        print("ERROR: No test plan found. Looking for TEST_IMPLEMENTATION_PLAN.md or TEST-COVERAGE-PLAN.md")
        return {}, {}
    
    with open(plan_path, 'r') as f:
        content = f.read()
    
    tasks = {}
    dependencies = defaultdict(list)
    
    # Parse tasks from phases
    phase_pattern = r'### Phase (\d+):[^#]+?(.*?)(?=###|\Z)'
    task_count = 0
    
    for phase_match in re.finditer(phase_pattern, content, re.DOTALL):
        phase_num = phase_match.group(1)
        phase_content = phase_match.group(2)
        
        # Extract tasks from checkbox items
        task_pattern = r'- \[ \] (.+?)(?:\n|$)'
        for task_match in re.finditer(task_pattern, phase_content):
            task_count += 1
            task_desc = task_match.group(1).strip()
            task_id = f"TASK-{task_count:03d}"
            
            # Determine priority based on phase
            if phase_num == "0":
                priority = "P0"
                milestone = "Critical Fixes"
            elif phase_num in ["1", "2"]:
                priority = "P1"
                milestone = "Core Testing"
            else:
                priority = "P2"
                milestone = "Advanced Testing"
            
            # Extract estimated time if present
            time_match = re.search(r'\((\d+[-\d]*)\s*(days?|hours?)\)', phase_content)
            estimated = time_match.group(0) if time_match else "2-4 hours"
            
            tasks[task_id] = {
                'id': task_id,
                'title': task_desc,
                'phase': f"Phase {phase_num}",
                'priority': priority,
                'milestone': milestone,
                'estimated': estimated,
                'status': 'pending'
            }
            
            # Simple dependency: tasks in later phases depend on earlier phases
            if int(phase_num) > 0:
                for i in range(1, task_count):
                    prev_task_id = f"TASK-{i:03d}"
                    if prev_task_id in tasks and tasks[prev_task_id]['phase'] < f"Phase {phase_num}":
                        dependencies[task_id].append(prev_task_id)
    
    return tasks, dependencies

def check_completed_tasks():
    """Check git commits and code for completed tasks"""
    completed = set()
    
    # Check for task branches or commits
    try:
        import subprocess
        # Get all branch names
        result = subprocess.run(['git', 'branch', '-a'], capture_output=True, text=True)
        branches = result.stdout.lower()
        
        # Get commit messages
        result = subprocess.run(['git', 'log', '--oneline', '-50'], capture_output=True, text=True)
        commits = result.stdout.lower()
        
        # Look for task references
        for task_id in range(1, 30):  # Check first 30 tasks
            task_str = f"task-{task_id:03d}"
            if task_str in branches or task_str in commits:
                completed.add(f"TASK-{task_id:03d}")
    except:
        pass  # Git not available or no commits yet
    
    # Check for expected test files from completed tasks
    artifacts = {
        'TASK-004': 'test/Tethys.MinimalEndpoints.ImprovedSourceGenerators.Tests/EquatableArrayTests.cs',
        'TASK-005': 'test/Tethys.MinimalEndpoints.ImprovedSourceGenerators.Tests/ModelEqualityTests.cs',
        'TASK-006': 'test/Tethys.MinimalEndpoints.Tests',
        'TASK-007': 'test/Tethys.MinimalEndpoints.ImprovedSourceGenerators.Tests/CacheabilityTests.cs',
    }
    
    for task_id, expected_path in artifacts.items():
        if Path(expected_path).exists():
            completed.add(task_id)
    
    # Check session notes for completed markers
    session_notes_path = Path(__file__).parent.parent / 'SESSION_NOTES.md'
    if session_notes_path.exists():
        with open(session_notes_path, 'r') as f:
            notes = f.read().lower()
            for i in range(1, 30):
                if f"task-{i:03d}" in notes and "completed" in notes:
                    completed.add(f"TASK-{i:03d}")
    
    return completed

def find_available_tasks(tasks, dependencies, completed):
    """Find tasks that can be started now"""
    available = []
    
    for task_id, task in tasks.items():
        if task_id in completed:
            continue
        
        # Check if all dependencies are completed
        task_deps = dependencies.get(task_id, [])
        if all(dep in completed for dep in task_deps):
            available.append(task)
    
    # Sort by phase and priority
    priority_order = {'P0': 0, 'P1': 1, 'P2': 2}
    
    available.sort(key=lambda t: (
        int(t['phase'].split()[-1]),  # Phase number
        priority_order.get(t['priority'], 99),
        t['id']
    ))
    
    return available

def print_task_board():
    """Print a task board showing current status"""
    tasks, dependencies = parse_tasks_from_plan()
    
    if not tasks:
        return
    
    completed = check_completed_tasks()
    
    # Update task statuses
    for task_id in completed:
        if task_id in tasks:
            tasks[task_id]['status'] = 'completed'
    
    available = find_available_tasks(tasks, dependencies, completed)
    
    # Group by phase
    by_phase = defaultdict(list)
    for task in tasks.values():
        by_phase[task['phase']].append(task)
    
    print("=" * 80)
    print("TETHYS TEST COVERAGE TASK STATUS")
    print("=" * 80)
    print()
    
    # Summary
    total = len(tasks)
    done = len(completed)
    in_progress = 0  # Could be detected from git branches
    pending = total - done - in_progress
    
    print(f"Total Tasks: {total} | Completed: {done} | In Progress: {in_progress} | Pending: {pending}")
    print(f"Progress: [{'█' * (done * 20 // total if total > 0 else 0)}{'░' * (20 - (done * 20 // total if total > 0 else 0))}] {done * 100 // total if total > 0 else 0}%")
    print()
    
    # Available tasks
    print("NEXT AVAILABLE TASKS:")
    print("-" * 80)
    if available:
        for i, task in enumerate(available[:5]):  # Show top 5
            deps = dependencies.get(task['id'], [])
            dep_str = f" (depends on: {', '.join(deps)})" if deps else ""
            print(f"{i+1}. [{task['priority']}] {task['id']}: {task['title']}")
            print(f"   Phase: {task['phase']} | Estimate: {task['estimated']}{dep_str}")
            print()
    else:
        print("No tasks available. Check if initial tasks are completed.")
    print()
    
    # Phase progress
    print("PHASE PROGRESS:")
    print("-" * 80)
    phase_order = ['Phase 0', 'Phase 1', 'Phase 2', 'Phase 3', 'Phase 4', 'Phase 5', 'Phase 6']
    
    for phase in phase_order:
        tasks_in_phase = by_phase.get(phase, [])
        if tasks_in_phase:
            completed_in_phase = sum(1 for t in tasks_in_phase if t['status'] == 'completed')
            total_in_phase = len(tasks_in_phase)
            percentage = completed_in_phase * 100 // total_in_phase if total_in_phase > 0 else 0
            
            phase_name = {
                'Phase 0': 'Critical Fixes',
                'Phase 1': 'Core Tests',
                'Phase 2': 'Cacheability',
                'Phase 3': 'Snapshots',
                'Phase 4': 'Performance',
                'Phase 5': 'Integration',
                'Phase 6': 'NuGet'
            }.get(phase, phase)
            
            print(f"{phase_name:20} [{completed_in_phase}/{total_in_phase}] {'█' * (percentage // 10)}{'░' * (10 - percentage // 10)} {percentage}%")
    
    print()
    print("QUICK COMMANDS:")
    print("-" * 80)
    print("Start a task:     git checkout -b task-XXX-description")
    print("Verify task:      ./scripts/verify-task.sh TASK-XXX")
    print("View task details: cat github-issues/task-XXX-*.md")
    print("Check coverage:   dotnet test /p:CollectCoverage=true")
    print()

def main():
    """Main entry point"""
    import sys
    
    if len(sys.argv) > 1 and sys.argv[1] == '--next':
        # Just show next task
        tasks, dependencies = parse_tasks_from_plan()
        completed = check_completed_tasks()
        available = find_available_tasks(tasks, dependencies, completed)
        
        if available:
            task = available[0]
            print(f"Next task: {task['id']} - {task['title']}")
            print(f"Priority: {task['priority']} | Phase: {task['phase']} | Estimate: {task['estimated']}")
        else:
            print("No tasks available")
    else:
        print_task_board()

if __name__ == "__main__":
    main()
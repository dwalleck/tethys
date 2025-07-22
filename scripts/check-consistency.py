#!/usr/bin/env python3
"""
Check consistency across Stratify documentation and code.
"""

import re
import os
from pathlib import Path
import xml.etree.ElementTree as ET

def check_package_versions():
    """Check NuGet package versions are consistent across projects"""
    print("Checking NuGet package versions...")

    packages = {}
    csproj_files = list(Path('.').rglob('*.csproj'))

    for csproj in csproj_files:
        if 'bin' in str(csproj) or 'obj' in str(csproj):
            continue

        try:
            tree = ET.parse(csproj)
            root = tree.getroot()

            for ref in root.findall('.//PackageReference'):
                name = ref.get('Include')
                version = ref.get('Version')
                if name and version:
                    if name not in packages:
                        packages[name] = {}
                    if version not in packages[name]:
                        packages[name][version] = []
                    packages[name][version].append(str(csproj))
        except:
            print(f"  ⚠️  Could not parse {csproj}")

    # Check for inconsistencies
    inconsistent = False
    for package, versions in packages.items():
        if len(versions) > 1:
            inconsistent = True
            print(f"  ❌ {package} has multiple versions:")
            for version, projects in versions.items():
                print(f"     - {version}: {', '.join(projects)}")

    if not inconsistent:
        print("  ✅ All package versions are consistent")

    return packages

def check_test_framework():
    """Ensure all test projects use TUnit"""
    print("\nChecking test framework usage...")

    test_projects = Path('test').rglob('*.csproj')
    issues = []

    for proj in test_projects:
        if 'bin' in str(proj) or 'obj' in str(proj):
            continue

        content = proj.read_text()

        # Check for xUnit references (except in Api.Tests which is legacy)
        if 'xunit' in content.lower() and 'Api.Tests' not in str(proj):
            issues.append(f"  ❌ {proj} uses xUnit instead of TUnit")

        # Check for TUnit
        if 'tunit' not in content.lower() and 'Api.Tests' not in str(proj):
            issues.append(f"  ⚠️  {proj} doesn't reference TUnit")

    if issues:
        for issue in issues:
            print(issue)
    else:
        print("  ✅ All test projects use correct framework")

def check_documentation_links():
    """Check that all referenced documentation files exist"""
    print("\nChecking documentation links...")

    md_files = list(Path('.').rglob('*.md'))
    missing_links = []

    for md_file in md_files:
        if 'node_modules' in str(md_file) or '.git' in str(md_file):
            continue

        content = md_file.read_text()

        # Find markdown links
        links = re.findall(r'\[([^\]]+)\]\(([^)]+)\)', content)

        for link_text, link_path in links:
            # Skip URLs and anchors
            if link_path.startswith('http') or link_path.startswith('#'):
                continue

            # Resolve relative path
            if not link_path.startswith('/'):
                full_path = md_file.parent / link_path
            else:
                full_path = Path('.') / link_path.lstrip('/')

            # Check if file exists
            if not full_path.exists():
                missing_links.append(f"  ❌ {md_file}: Link to '{link_path}' not found")

    if missing_links:
        for link in missing_links[:10]:  # Show first 10
            print(link)
        if len(missing_links) > 10:
            print(f"  ... and {len(missing_links) - 10} more")
    else:
        print("  ✅ All documentation links are valid")

def check_test_coverage_goals():
    """Check if coverage goals are documented and consistent"""
    print("\nChecking test coverage goals...")

    coverage_mentions = {}

    # Check various documentation files
    doc_files = ['AGENT-BOOTSTRAP.md', 'TEST_STRATEGY.md', 'TEST_IMPLEMENTATION_PLAN.md', 'IMPROVE_TEST_COVERAGE.md']

    for doc in doc_files:
        if Path(doc).exists():
            content = Path(doc).read_text()

            # Find coverage percentages mentioned
            percentages = re.findall(r'(\d+)%\s*coverage', content, re.IGNORECASE)
            if percentages:
                coverage_mentions[doc] = set(percentages)

    # Check if they're consistent
    all_percentages = set()
    for percentages in coverage_mentions.values():
        all_percentages.update(percentages)

    if len(all_percentages) > 2:  # Allow for some variation
        print("  ⚠️  Multiple coverage targets found:")
        for doc, percentages in coverage_mentions.items():
            print(f"     - {doc}: {', '.join(sorted(percentages))}%")
    else:
        print(f"  ✅ Coverage goals are consistent: {', '.join(sorted(all_percentages))}%")

def check_source_generator_patterns():
    """Check that source generator follows documented patterns"""
    print("\nChecking source generator patterns...")

    gen_file = Path('src/Stratify.MinimalEndpoints.ImprovedSourceGenerators/EndpointGeneratorImproved.cs')

    if gen_file.exists():
        content = gen_file.read_text()

        issues = []

        # Check for required patterns
        if 'ForAttributeWithMetadataName' not in content:
            issues.append("  ❌ Not using ForAttributeWithMetadataName for efficiency")

        if 'IIncrementalGenerator' not in content:
            issues.append("  ❌ Not implementing IIncrementalGenerator")

        if 'TrackingNames' not in content:
            issues.append("  ⚠️  No tracking names for debugging")

        if issues:
            for issue in issues:
                print(issue)
        else:
            print("  ✅ Source generator follows documented patterns")
    else:
        print("  ❌ Source generator file not found")

def main():
    """Run all consistency checks"""
    print("=== Stratify Consistency Check ===\n")

    # Change to project root
    script_dir = Path(__file__).parent
    os.chdir(script_dir.parent)

    check_package_versions()
    check_test_framework()
    check_documentation_links()
    check_test_coverage_goals()
    check_source_generator_patterns()

    print("\n=== Check Complete ===")
    print("\nIf any issues were found, fix them before proceeding with tasks.")

if __name__ == "__main__":
    main()

# MCP (Model Context Protocol) Usage Guide for Stratify Development

## Overview

MCP tools provide enhanced capabilities beyond standard tools. This guide ensures agents use MCP tools effectively, especially for package documentation and compilation issues.

## Critical Rule: Always Check Documentation First

**BEFORE writing any code that uses external packages, ALWAYS use context7 to get current documentation.**

## Available MCP Tools

### 1. context7 - Library Documentation (HIGHEST PRIORITY)

**Purpose**: Get up-to-date documentation for any NuGet package or library.

**ALWAYS use context7 for**:

- Any compilation error involving external packages
- Before using any NuGet package API
- When unsure about method signatures
- To find code examples and patterns
- To check for breaking changes between versions

**Usage Pattern**:

```bash
# Step 1: Resolve library name to ID
mcp__context7__resolve-library-id --libraryName "FluentValidation"

# Step 2: Get documentation
mcp__context7__get-library-docs --context7CompatibleLibraryID "/fluentvalidation/fluentvalidation"
```

### 2. github - Repository Operations

**Use for**:

- Creating issues with proper labels
- Managing pull requests
- Updating repository settings

### 3. fetch - Web Content

**Use for**:

- Fetching online documentation
- Retrieving web resources
- Preferred over WebFetch

### 4. desktop-commander - Advanced Operations

**Use for**:

- Complex file operations
- Process management
- System-level tasks

## Common Scenarios & Required Actions

### Scenario 1: Compilation Error with External Package

```
Error CS1061: 'IValidator<T>' does not contain a definition for 'ValidateAsync'
```

**REQUIRED ACTIONS**:

1. Immediately use context7 to check FluentValidation documentation
2. Verify the exact method name and parameters
3. Check namespace requirements
4. Look for version-specific changes

### Scenario 2: Implementing with Unfamiliar Package

Before writing:

```csharp
services.AddFluentValidation(fv => fv.RegisterValidatorsFromAssembly(assembly));
```

**REQUIRED ACTIONS**:

1. Use context7 to get FluentValidation.DependencyInjectionExtensions docs
2. Verify the registration method exists
3. Check for the recommended approach in current version
4. Look for configuration examples

### Scenario 3: Type Conversion Issues

```
Error CS0029: Cannot implicitly convert type 'ValidationResult' to 'FluentValidation.Results.ValidationResult'
```

**REQUIRED ACTIONS**:

1. Use context7 to check both types
2. Verify namespace differences
3. Look for conversion examples
4. Check for extension methods

## Package-Specific Guidelines

### FluentValidation

- Always check current syntax for validators
- Verify DI registration patterns
- Check for async validation methods

### Entity Framework Core

- Verify DbContext configuration syntax
- Check migration command changes
- Verify LINQ query compatibility

### TUnit (Testing Framework)

- Verify attribute names and namespaces
- Check assertion syntax
- Look for async test patterns

### Verify.TUnit (Snapshot Testing)

- Check initialization requirements
- Verify settings configuration
- Look for serialization options

## Integration with Development Workflow

### 1. Starting a Task

```
Read task requirements
↓
Identify external packages used
↓
Use context7 for EACH package
↓
Begin implementation
```

### 2. Hitting Compilation Errors

```
Compilation error occurs
↓
Is it package-related?
├─ Yes → Use context7 immediately
└─ No → Standard debugging
```

### 3. Code Review Checklist

- [ ] All package APIs verified with context7?
- [ ] Method signatures match documentation?
- [ ] Using current best practices?
- [ ] No deprecated APIs used?

## Common Mistakes to Avoid

### ❌ DON'T: Guess at API signatures

```csharp
// Wrong: Guessing the method exists
validator.ValidateAsync(model); // Might not exist!
```

### ✅ DO: Verify with context7 first

```csharp
// Right: After checking context7
await validator.ValidateAsync(model, cancellationToken);
```

### ❌ DON'T: Assume methods from old versions

```csharp
// Wrong: Using old registration pattern
services.AddFluentValidation(); // Deprecated!
```

### ✅ DO: Check current patterns

```csharp
// Right: Current pattern from context7
services.AddValidatorsFromAssembly(Assembly.GetExecutingAssembly());
```

## Emergency Procedures

### Package Method Not Found

1. Stop coding immediately
2. Use context7 to verify the method exists
3. Check you have the correct package version
4. Verify namespace imports
5. Look for renamed/moved methods

### Type Mismatch with Package Types

1. Use context7 to check both types
2. Verify you're using consistent package versions
3. Check for namespace conflicts
4. Look for conversion methods

### Package Behavior Unexpected

1. Check context7 for behavior documentation
2. Verify version-specific changes
3. Look for migration guides
4. Check for known issues

## Quick Reference Card

| Situation | Action | MCP Tool |
|-----------|--------|----------|
| Using new package | Get docs first | context7 |
| Method not found | Verify signature | context7 |
| Type conversion error | Check both types | context7 |
| Creating GitHub issues | Use MCP | github |
| Fetching web docs | Use MCP | fetch |
| Complex file ops | Use MCP | desktop-commander |

## Golden Rules

1. **Never guess** - Always verify with documentation
2. **Check first** - Use context7 before writing code with packages
3. **Version matters** - Package APIs change between versions
4. **Examples help** - Look for official code examples
5. **Current is key** - Use current patterns, not outdated ones

## Integration with AGENT-BOOTSTRAP.md

This guide complements the agent bootstrap process. Add these checks:

### Before Writing Code

- [ ] Identified all external packages
- [ ] Used context7 for each package
- [ ] Verified all APIs exist
- [ ] Checked for best practices

### During Development

- [ ] Compilation error? Check context7 first
- [ ] Type issues? Verify with context7
- [ ] Unsure? Stop and check context7

### After Implementation

- [ ] All package usage verified
- [ ] No deprecated APIs used
- [ ] Following current patterns
- [ ] Tests use verified APIs

Remember: **When in doubt, check documentation with context7!**

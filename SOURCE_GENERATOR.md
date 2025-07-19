# Source Generator Comprehensive Guide - Andrew Lock's Series Summary

This document summarizes Andrew Lock's 14-part series on creating source generators, with a special focus on testing strategies.

## Series Overview

Andrew Lock's series covers creating incremental source generators using the APIs introduced in .NET 6. The series demonstrates building an `EnumExtensions` generator that creates a `ToStringFast()` method for enums, which is significantly faster than the built-in `ToString()` method.

## Table of Contents

1. [Creating an Incremental Generator](#creating-an-incremental-generator)
2. [Testing Strategies](#testing-strategies)
   - [Snapshot Testing](#snapshot-testing)
   - [Integration Testing](#integration-testing)
   - [Cacheability Testing](#cacheability-testing)
3. [Performance Best Practices](#performance-best-practices)
4. [Advanced Topics](#advanced-topics)
5. [Key Takeaways](#key-takeaways)

## Creating an Incremental Generator

### Basic Setup

1. **Project Configuration**
```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>netstandard2.0</TargetFramework>
    <IncludeBuildOutput>false</IncludeBuildOutput>
    <Nullable>enable</Nullable>
    <ImplicitUsings>true</ImplicitUsings>
    <LangVersion>Latest</LangVersion>
  </PropertyGroup>
  
  <ItemGroup>
    <PackageReference Include="Microsoft.CodeAnalysis.Analyzers" Version="3.3.2" PrivateAssets="all" />
    <PackageReference Include="Microsoft.CodeAnalysis.CSharp" Version="4.0.1" PrivateAssets="all" />
  </ItemGroup>
</Project>
```

2. **Basic Generator Structure**
```csharp
[Generator]
public class EnumGenerator : IIncrementalGenerator
{
    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        // Register marker attribute
        context.RegisterPostInitializationOutput(ctx => ctx.AddSource(
            "EnumExtensionsAttribute.g.cs",
            SourceText.From(SourceGenerationHelper.Attribute, Encoding.UTF8)));

        // Build pipeline
        IncrementalValuesProvider<EnumToGenerate?> enumsToGenerate = context.SyntaxProvider
            .ForAttributeWithMetadataName(
                "NetEscapades.EnumGenerators.EnumExtensionsAttribute",
                predicate: static (node, _) => node is EnumDeclarationSyntax,
                transform: static (ctx, _) => GetSemanticTargetForGeneration(ctx))
            .Where(static m => m is not null);

        // Generate source
        context.RegisterSourceOutput(enumsToGenerate, 
            static (spc, source) => Execute(source, spc));
    }
}
```

## Testing Strategies

### Snapshot Testing

**Part 2** focuses on using snapshot testing with the Verify library for testing source generators.

#### Setup
```xml
<ItemGroup>
  <PackageReference Include="Verify.TUnit" Version="28.4.0" />
  <PackageReference Include="Verify.SourceGenerators" Version="1.2.0" />
  <PackageReference Include="Microsoft.CodeAnalysis.CSharp" Version="4.0.1" PrivateAssets="all" />
</ItemGroup>
```

#### Test Implementation
```csharp
[UsesVerify]
public class EnumGeneratorSnapshotTests
{
    [Test]
    public Task GeneratesEnumExtensionsCorrectly()
    {
        var source = @"
using NetEscapades.EnumGenerators;

[EnumExtensions]
public enum Colour
{
    Red = 0,
    Blue = 1,
}";

        return TestHelper.Verify(source);
    }
}
```

#### Test Helper
```csharp
public static class TestHelper
{
    public static Task Verify(string source)
    {
        SyntaxTree syntaxTree = CSharpSyntaxTree.ParseText(source);
        
        IEnumerable<PortableExecutableReference> references = new[]
        {
            MetadataReference.CreateFromFile(typeof(object).Assembly.Location)
        };

        CSharpCompilation compilation = CSharpCompilation.Create(
            assemblyName: "Tests",
            syntaxTrees: new[] { syntaxTree },
            references: references);

        var generator = new EnumGenerator();
        GeneratorDriver driver = CSharpGeneratorDriver.Create(generator);
        driver = driver.RunGenerators(compilation);

        return Verifier.Verify(driver);
    }
}
```

#### Module Initializer (Required for Verify)
```csharp
public static class ModuleInitializer
{
    [ModuleInitializer]
    public static void Init()
    {
        VerifySourceGenerators.Enable();
    }
}
```

### Integration Testing

**Part 3** covers integration testing and NuGet packaging.

#### Integration Test Project Setup
```xml
<ProjectReference Include="..\..\src\NetEscapades.EnumGenerators\NetEscapades.EnumGenerators.csproj" 
                  OutputItemType="Analyzer" 
                  ReferenceOutputAssembly="false" />
```

#### Integration Test Example
```csharp
using TUnit.Core;
using TUnit.Assertions;
using TUnit.Assertions.Extensions;

public class EnumExtensionsTests
{
    [Test]
    [Arguments(Colour.Red)]
    [Arguments(Colour.Green)]
    [Arguments(Colour.Green | Colour.Blue)]
    [Arguments((Colour)15)]
    [Arguments((Colour)0)]
    public async Task FastToStringIsSameAsToString(Colour value)
    {
        var expected = value.ToString();
        var actual = value.ToStringFast();
        await Assert.That(actual).IsEqualTo(expected);
    }
}
```

#### NuGet Package Testing
```bash
# Create local NuGet config
dotnet new nugetconfig
mv nuget.config nuget.integration-tests.config
dotnet nuget add source ./artifacts -n local-packages --configfile nuget.integration-tests.config

# Test NuGet package
dotnet restore ./tests/NetEscapades.EnumGenerators.NugetIntegrationTests --packages ./packages --configfile "nuget.integration-tests.config"
dotnet build ./tests/NetEscapades.EnumGenerators.NugetIntegrationTests -c Release --packages ./packages --no-restore
dotnet test ./tests/NetEscapades.EnumGenerators.NugetIntegrationTests -c Release --no-build --no-restore
```

### Cacheability Testing

**Part 10** introduces advanced testing for ensuring incremental generator outputs are cacheable.

#### Adding Tracking Names
```csharp
internal class TrackingNames
{
    public const string InitialExtraction = nameof(InitialExtraction);
    public const string Transform = nameof(Transform);
}

// In generator
IncrementalValuesProvider<EnumDetails?> enumDetails = context.SyntaxProvider
    .ForAttributeWithMetadataName(/* ... */)
    .WithTrackingName(TrackingNames.InitialExtraction);

IncrementalValuesProvider<EnumToGenerate> valuesToGenerate = enumDetails
    .Select(static (value, _) => ConvertValue(value))
    .WithTrackingName(TrackingNames.Transform);
```

#### Cacheability Test Helper
```csharp
private static GeneratorDriverRunResult RunGeneratorAndAssertOutput<T>(
    CSharpCompilation compilation, 
    string[] trackingNames, 
    bool assertOutput = true) where T : IIncrementalGenerator, new()
{
    ISourceGenerator generator = new T().AsSourceGenerator();
    
    var opts = new GeneratorDriverOptions(
        disabledOutputs: IncrementalGeneratorOutputKind.None,
        trackIncrementalGeneratorSteps: true);

    GeneratorDriver driver = CSharpGeneratorDriver.Create([generator], driverOptions: opts);
    
    var clone = compilation.Clone();
    driver = driver.RunGenerators(compilation);
    GeneratorDriverRunResult runResult = driver.GetRunResult();

    if (assertOutput)
    {
        GeneratorDriverRunResult runResult2 = driver
            .RunGenerators(clone)
            .GetRunResult();

        AssertRunsEqual(runResult, runResult2, trackingNames);
        
        // Verify all outputs are cached
        runResult2.Results[0]
            .TrackedOutputSteps
            .SelectMany(x => x.Value)
            .SelectMany(x => x.Outputs)
            .Should()
            .OnlyContain(x => x.Reason == IncrementalStepRunReason.Cached);  // Note: May need TUnit assertion equivalent
    }

    return runResult;
}
```

## Performance Best Practices

### From Part 9: Avoiding Performance Pitfalls

1. **Use ForAttributeWithMetadataName (NET 7+)**
   - Can remove 99% of nodes evaluated
   - Requires .NET 7 SDK and Microsoft.CodeAnalysis.CSharp 4.4.0+

2. **Don't use Syntax or ISymbol in pipeline**
   - Always convert to a value-based data model
   - These types break caching

3. **Use value types or records**
   ```csharp
   public readonly record struct EnumToGenerate
   {
       public readonly string Name;
       public readonly EquatableArray<string> Values;
   }
   ```

4. **Watch out for collection types**
   - Standard collections don't implement structural equality
   - Use custom `EquatableArray<T>` implementation

5. **Be careful with CompilationProvider**
   - Don't combine with your main pipeline
   - It breaks caching benefits

6. **Handle diagnostics carefully**
   - Create diagnostics once and cache them
   - Don't create new instances repeatedly

7. **Consider RegisterImplementationSourceOutput**
   - Use for static code that doesn't change
   - More efficient than RegisterSourceOutput

## Advanced Topics

### Part 4: Customizing with Marker Attributes
```csharp
[EnumExtensions(ExtensionClassName = "DirectionExtensions")]
public enum Direction { Left, Right, Up, Down }
```

### Part 5: Finding Namespace and Type Hierarchy
- Handling nested types
- Resolving full namespace paths

### Part 6: Saving Source Generator Output
- Debugging generated code
- Source control considerations

### Parts 7-8: Marker Attribute Problems
- Avoiding compilation issues with marker attributes
- Alternative approaches to marking types

### Part 11: Implementing Interceptors
- Using source generators with C# 12 interceptors

### Part 12: Reading Compilation Options
- Accessing C# version
- Reading project configuration

### Part 13: Accessing MSBuild Properties
- Reading custom MSBuild properties
- User configuration options

### Part 14: Supporting Multiple SDK Versions
- Multi-targeting different Roslyn versions
- Conditional compilation strategies

## Key Takeaways

1. **Testing is Critical**
   - Snapshot testing provides quick feedback
   - Integration testing ensures real-world functionality
   - Cacheability testing prevents performance issues

2. **Performance Matters**
   - Source generators run on every keystroke
   - Poor performance impacts IDE experience
   - Always use value-based data models

3. **Use Modern APIs**
   - ForAttributeWithMetadataName for attribute-based generators
   - Incremental generators over ISourceGenerator
   - Track outputs for testing

4. **Test at Multiple Levels**
   - Unit tests with snapshot testing
   - Integration tests with project references
   - NuGet package tests for distribution
   - Performance tests for cacheability

5. **Common Pitfalls to Avoid**
   - Using Syntax/ISymbol types in pipeline
   - Not implementing value equality
   - Ignoring collection equality
   - Mixing CompilationProvider with main pipeline

This guide provides a comprehensive overview of creating and testing source generators based on Andrew Lock's extensive series. The focus on testing ensures your generators are reliable, performant, and maintainable.
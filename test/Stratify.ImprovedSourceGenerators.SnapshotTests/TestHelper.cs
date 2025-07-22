using System.Collections.Immutable;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Stratify.MinimalEndpoints.Attributes;
using Stratify.MinimalEndpoints.ImprovedSourceGenerators;
using VerifyTests;
using VerifyTUnit;

namespace Stratify.ImprovedSourceGenerators.SnapshotTests;

public static class TestHelper
{
    /// <summary>
    /// Verifies the output of the EndpointGeneratorImproved for given source code
    /// </summary>
    public static SettingsTask Verify(string source, VerifySettings? settings = null)
    {
        var syntaxTree = CSharpSyntaxTree.ParseText(source);

        var references = GetStandardReferences();

        var compilation = CSharpCompilation.Create(
            assemblyName: "Tests",
            syntaxTrees: new[] { syntaxTree },
            references: references,
            options: new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));

        var generator = new EndpointGeneratorImproved();

        GeneratorDriver driver = CSharpGeneratorDriver.Create(generator);
        driver = driver.RunGenerators(compilation);

        return Verifier.Verify(driver, settings);
    }

    /// <summary>
    /// Verifies the output with tracking names for debugging incremental compilation
    /// </summary>
    public static SettingsTask VerifyWithTracking(string source, VerifySettings? settings = null)
    {
        var syntaxTree = CSharpSyntaxTree.ParseText(source);

        var references = GetStandardReferences();

        var compilation = CSharpCompilation.Create(
            assemblyName: "Tests",
            syntaxTrees: new[] { syntaxTree },
            references: references,
            options: new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));

        var generator = new EndpointGeneratorImproved();

        // Enable tracking for incremental compilation debugging
        var driverOptions = new GeneratorDriverOptions(
            disabledOutputs: IncrementalGeneratorOutputKind.None,
            trackIncrementalGeneratorSteps: true);

        GeneratorDriver driver = CSharpGeneratorDriver.Create([generator.AsSourceGenerator()], driverOptions: driverOptions);
        driver = driver.RunGenerators(compilation);

        return Verifier.Verify(driver, settings);
    }

    /// <summary>
    /// Gets the results of running the generator for cacheability testing
    /// </summary>
    public static GeneratorDriverRunResult GetGeneratorRunResult(
        string source,
        bool trackIncrementalSteps = false)
    {
        var syntaxTree = CSharpSyntaxTree.ParseText(source);

        var references = GetStandardReferences();

        var compilation = CSharpCompilation.Create(
            assemblyName: "Tests",
            syntaxTrees: new[] { syntaxTree },
            references: references,
            options: new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));

        var generator = new EndpointGeneratorImproved();

        GeneratorDriverOptions? driverOptions = null;
        if (trackIncrementalSteps)
        {
            driverOptions = new GeneratorDriverOptions(
                disabledOutputs: IncrementalGeneratorOutputKind.None,
                trackIncrementalGeneratorSteps: true);
        }

        GeneratorDriver driver = driverOptions is null
            ? CSharpGeneratorDriver.Create(generator.AsSourceGenerator())
            : CSharpGeneratorDriver.Create([generator.AsSourceGenerator()], driverOptions: driverOptions.Value);

        driver = driver.RunGenerators(compilation);

        return driver.GetRunResult();
    }

    /// <summary>
    /// Gets standard references including ASP.NET Core types
    /// </summary>
    private static ImmutableArray<PortableExecutableReference> GetStandardReferences()
    {
        var references = new List<PortableExecutableReference>
        {
            // Core references
            MetadataReference.CreateFromFile(typeof(object).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(Enumerable).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(Task).Assembly.Location),
            MetadataReference.CreateFromFile(typeof(Attribute).Assembly.Location),

            // Get reference to our base library
            MetadataReference.CreateFromFile(typeof(EndpointAttribute).Assembly.Location)
        };

        // Add ASP.NET Core types from a predictable location
        var aspNetCoreAssembly = AppDomain.CurrentDomain.GetAssemblies()
            .FirstOrDefault(a => a.GetName().Name == "Microsoft.AspNetCore.Http.Abstractions");

        if (aspNetCoreAssembly != null)
        {
            references.Add(MetadataReference.CreateFromFile(aspNetCoreAssembly.Location));
        }

        // Add runtime assembly
        var runtimeAssembly = AppDomain.CurrentDomain.GetAssemblies()
            .FirstOrDefault(a => a.GetName().Name == "System.Runtime");

        if (runtimeAssembly != null)
        {
            references.Add(MetadataReference.CreateFromFile(runtimeAssembly.Location));
        }

        return references.ToImmutableArray();
    }

    /// <summary>
    /// Creates a compilation with multiple source files
    /// </summary>
    public static CSharpCompilation CreateCompilation(params string[] sources)
    {
        var syntaxTrees = sources.Select(source => CSharpSyntaxTree.ParseText(source)).ToArray();

        return CSharpCompilation.Create(
            assemblyName: "Tests",
            syntaxTrees: syntaxTrees,
            references: GetStandardReferences(),
            options: new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary));
    }
}

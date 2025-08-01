using System.Collections.Immutable;
using Microsoft.CodeAnalysis;
using TUnit.Assertions;
using TUnit.Core;

namespace Stratify.ImprovedSourceGenerators.SnapshotTests;

public class CacheabilityTests
{
    [Test]
    public async Task Generator_OutputsAreCacheable_WhenNoChangesAreMade()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/test")]
            public partial class TestEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok());
                }
            }
            """;

        // Act - Run generator twice on the same source
        var firstRun = TestHelper.GetGeneratorRunResult(source, trackIncrementalSteps: true);
        var secondRun = TestHelper.GetGeneratorRunResult(source, trackIncrementalSteps: true);

        // Assert - All outputs should be cached on second run
        var firstSteps = GetTrackedSteps(firstRun);
        var secondSteps = GetTrackedSteps(secondRun);

        // First run should have executed steps
        await Assert.That(firstSteps).IsNotEmpty();
        await Assert.That(firstSteps.Any(step =>
            step.Value.Any(s => s.Outputs.Any(output => output.Reason == IncrementalStepRunReason.New)))).IsTrue();

        // Second run should have all cached steps
        await Assert.That(secondSteps).IsNotEmpty();
        var allReasons = secondSteps.SelectMany(step => step.Value)
            .SelectMany(s => s.Outputs)
            .Select(output => output.Reason)
            .ToList();
        await Assert.That(allReasons.All(reason => reason == IncrementalStepRunReason.Cached)).IsTrue();

        // Verify no compilation errors
        await Assert.That(firstRun.Diagnostics.Where(d => d.Severity == DiagnosticSeverity.Error)).IsEmpty();
        await Assert.That(secondRun.Diagnostics.Where(d => d.Severity == DiagnosticSeverity.Error)).IsEmpty();
    }

    [Test]
    public async Task Generator_RerunsOnlyAffectedNodes_WhenUnrelatedCodeChanges()
    {
        // Arrange
        var sourceWithEndpoint = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/test")]
            public partial class TestEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok());
                }
            }
            """;

        var sourceWithUnrelatedClass = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/test")]
            public partial class TestEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok());
                }
            }

            // This class should not affect the generator
            public class UnrelatedClass
            {
                public string Property { get; set; } = "";
            }
            """;

        // Act
        var firstRun = TestHelper.GetGeneratorRunResult(sourceWithEndpoint, trackIncrementalSteps: true);
        var secondRun = TestHelper.GetGeneratorRunResult(sourceWithUnrelatedClass, trackIncrementalSteps: true);

        // Assert - Generated output should be the same
        var firstOutput = GetGeneratedOutput(firstRun);
        var secondOutput = GetGeneratedOutput(secondRun);

        await Assert.That(firstOutput).IsEqualTo(secondOutput);

        // Most steps should still be cached
        var secondSteps = GetTrackedSteps(secondRun);
        var cachedSteps = secondSteps
            .SelectMany(step => step.Value)
            .SelectMany(s => s.Outputs)
            .Count(output => output.Reason == IncrementalStepRunReason.Cached);

        await Assert.That(cachedSteps).IsGreaterThan(0);
    }

    [Test]
    public async Task Generator_RerunsAffectedNodes_WhenEndpointChanges()
    {
        // Arrange
        var originalSource = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/test")]
            public partial class TestEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok());
                }
            }
            """;

        var modifiedSource = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Post, "/api/test")] // Changed from GET to POST
            public partial class TestEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok());
                }
            }
            """;

        // Act
        var firstRun = TestHelper.GetGeneratorRunResult(originalSource, trackIncrementalSteps: true);
        var secondRun = TestHelper.GetGeneratorRunResult(modifiedSource, trackIncrementalSteps: true);

        // Assert - Output should change
        var firstOutput = GetGeneratedOutput(firstRun);
        var secondOutput = GetGeneratedOutput(secondRun);

        await Assert.That(firstOutput).Contains("MapGet");
        await Assert.That(secondOutput).Contains("MapPost");
        await Assert.That(firstOutput).IsNotEqualTo(secondOutput);

        // Some steps should have rerun
        var secondSteps = GetTrackedSteps(secondRun);
        var hasNonCachedSteps = secondSteps.SelectMany(step => step.Value)
            .SelectMany(s => s.Outputs)
            .Any(output => output.Reason != IncrementalStepRunReason.Cached);
        await Assert.That(hasNonCachedSteps).IsTrue();
    }

    [Test]
    public Task VerifyTrackingNames_AreProperlyConfigured()
    {
        // This test verifies that tracking names are set up for debugging
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints.Attributes;

            namespace TestApp;

            [Endpoint(HttpMethodType.Get, "/api/test")]
            public partial class TestEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync()
                {
                    return Task.FromResult(Results.Ok());
                }
            }
            """;

        return TestHelper.VerifyWithTracking(source);
    }

    // Helper methods
    private static IEnumerable<(string Key, ImmutableArray<IncrementalGeneratorRunStep> Value)> GetTrackedSteps(GeneratorDriverRunResult runResult)
    {
        return runResult.Results[0]
            .TrackedOutputSteps
            .Select(kvp => (kvp.Key, kvp.Value));
    }

    private static string GetGeneratedOutput(GeneratorDriverRunResult runResult)
    {
        var generatedSource = runResult.Results[0]
            .GeneratedSources
            .FirstOrDefault();

        return generatedSource.SourceText?.ToString() ?? string.Empty;
    }
}

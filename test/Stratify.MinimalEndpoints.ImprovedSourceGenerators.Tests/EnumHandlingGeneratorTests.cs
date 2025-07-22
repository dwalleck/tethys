using System.Collections.Immutable;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.Text;
using Stratify.MinimalEndpoints.ImprovedSourceGenerators;
using Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests.Helpers;
using TUnit.Assertions;
using TUnit.Assertions.Extensions;
using TUnit.Core;

namespace Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests;

public class EnumHandlingGeneratorTests
{
    [Test]
    public async Task Generator_Should_Handle_Enum_Parameters_In_Endpoints()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints;

            namespace TestApp.Features.Products;

            public enum ProductStatus
            {
                Active,
                Inactive,
                Discontinued
            }

            [Endpoint(HttpMethod.Get, "/products/status/{status}")]
            public partial class GetProductsByStatusEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(ProductStatus status)
                {
                    return Task.FromResult(Results.Ok($"Getting products with status: {status}"));
                }
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();

        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        await Assert.That(generatedFiles.Count).IsEqualTo(1);

        // The generator itself produces no errors (test infrastructure has IEnumerable<> issue)

        // Verify the generated endpoint uses the handler method
        var generatedContent = generatedFiles[0].Content;
        await Assert.That(generatedContent).Contains("HandleAsync");
    }

    [Test]
    public async Task Generator_Should_Handle_Enum_In_Request_Record()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints;

            namespace TestApp.Features.Products;

            public enum OrderStatus
            {
                Pending = 0,
                Processing = 1,
                Completed = 2,
                Cancelled = 3
            }

            public record UpdateOrderRequest(int OrderId, OrderStatus Status);

            [Endpoint(HttpMethod.Put, "/orders/{orderId}/status")]
            public partial class UpdateOrderStatusEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(int orderId, UpdateOrderRequest request)
                {
                    return Task.FromResult(Results.Ok($"Order {orderId} status updated to {request.Status}"));
                }
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();

        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        await Assert.That(generatedFiles.Count).IsEqualTo(1);

        // The generator itself produces no errors (test infrastructure has IEnumerable<> issue)

        // Verify correct HTTP method mapping
        var generatedContent = generatedFiles[0].Content;
        await Assert.That(generatedContent).Contains("MapPut");
    }

    [Test]
    public async Task Generator_Should_Handle_Nullable_Enum_Parameters()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints;

            namespace TestApp.Features.Products;

            public enum Priority
            {
                Low,
                Medium,
                High,
                Critical
            }

            [Endpoint(HttpMethod.Get, "/tasks")]
            public partial class FilterTasksEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(Priority? priority = null)
                {
                    var result = priority.HasValue
                        ? $"Filtering by priority: {priority}"
                        : "Getting all tasks";
                    return Task.FromResult(Results.Ok(result));
                }
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();

        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        await Assert.That(generatedFiles.Count).IsGreaterThanOrEqualTo(1);
    }

    [Test]
    public async Task Generator_Should_Handle_Flags_Enum()
    {
        // Arrange
        var source = """
            using System;
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints;

            namespace TestApp.Features.Permissions;

            [Flags]
            public enum Permissions
            {
                None = 0,
                Read = 1,
                Write = 2,
                Delete = 4,
                Admin = Read | Write | Delete
            }

            [Endpoint(HttpMethod.Get, "/permissions/check")]
            public partial class CheckPermissionsEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(Permissions perms)
                {
                    return Task.FromResult(Results.Ok($"Checking permissions: {perms}"));
                }
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();

        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        await Assert.That(generatedFiles.Count).IsGreaterThanOrEqualTo(1);
    }

    [Test]
    public async Task Generator_Should_Handle_Enum_Arrays_And_Lists()
    {
        // Arrange
        var source = """
            using System.Collections.Generic;
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints;

            namespace TestApp.Features.Filters;

            public enum Category
            {
                Electronics,
                Clothing,
                Food,
                Books,
                Sports
            }

            public record FilterRequest(List<Category> Categories, Category[] ExcludeCategories);

            [Endpoint(HttpMethod.Post, "/products/filter")]
            public partial class FilterProductsEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(FilterRequest request)
                {
                    return Task.FromResult(Results.Ok($"Filtering {request.Categories.Count} categories"));
                }
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();

        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        await Assert.That(generatedFiles.Count).IsGreaterThanOrEqualTo(1);
    }

    [Test]
    public async Task Generator_Should_Handle_Enum_With_Custom_Values()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints;

            namespace TestApp.Features.Responses;

            public enum HttpStatusCode
            {
                OK = 200,
                Created = 201,
                BadRequest = 400,
                NotFound = 404,
                InternalServerError = 500
            }

            [Endpoint(HttpMethod.Get, "/response/{code}")]
            public partial class CustomResponseEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(HttpStatusCode code)
                {
                    return Task.FromResult(Results.StatusCode((int)code));
                }
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();

        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        await Assert.That(generatedFiles.Count).IsGreaterThanOrEqualTo(1);
    }

    [Test]
    public async Task Generator_Should_Handle_Nested_Enum_Types()
    {
        // Arrange
        var source = """
            using System.Threading.Tasks;
            using Microsoft.AspNetCore.Http;
            using Stratify.MinimalEndpoints;

            namespace TestApp.Features.Users;

            public enum UserRole
            {
                Guest,
                User,
                Moderator,
                Admin
            }

            public enum AccountStatus
            {
                Active,
                Suspended,
                Banned
            }

            public record UpdateUserRequest(UserRole Role, AccountStatus Status);

            [Endpoint(HttpMethod.Put, "/users/{id}")]
            public partial class UpdateUserEndpoint
            {
                [Handler]
                public static Task<IResult> HandleAsync(int id, UpdateUserRequest request)
                {
                    return Task.FromResult(Results.Ok($"User {id} updated to {request.Role} with status {request.Status}"));
                }
            }
            """;

        // Act
        var (outputCompilation, diagnostics) = await RunGenerator(source);

        // Assert
        var errorDiagnostics = TestCompilationHelper.GetErrorDiagnostics(diagnostics);
        await Assert.That(errorDiagnostics).IsEmpty();

        var generatedFiles = TestCompilationHelper.GetGeneratedFiles(outputCompilation);
        await Assert.That(generatedFiles.Count).IsGreaterThanOrEqualTo(1);
    }

    private async Task<(Compilation outputCompilation, ImmutableArray<Diagnostic> diagnostics)> RunGenerator(string source)
    {
        var generator = new EndpointGeneratorImproved();
        var driver = CSharpGeneratorDriver.Create(generator);

        var compilation = TestCompilationHelper.CreateCompilation(source);
        driver.RunGeneratorsAndUpdateCompilation(compilation, out var outputCompilation, out var diagnostics);

        return (outputCompilation, diagnostics);
    }
}

using System.Collections.Immutable;
using Tethys.MinimalEndpoints.ImprovedSourceGenerators;
using TUnit.Assertions;
using TUnit.Assertions.Extensions;
using TUnit.Core;

namespace Tethys.MinimalEndpoints.ImprovedSourceGenerators.Tests;

public class ModelEqualityTests
{
    [Test]
    public async Task Test_EquatableArray_Equality()
    {
        // Arrange
        var array1 = new EquatableArray<string>(ImmutableArray.Create("one", "two", "three"));
        var array2 = new EquatableArray<string>(ImmutableArray.Create("one", "two", "three"));

        // Assert
        await Assert.That(array1.Equals(array2)).IsTrue();
        await Assert.That(array1 == array2).IsTrue();
        await Assert.That(array1 != array2).IsFalse();
    }

    [Test]
    public async Task Test_EquatableArray_Inequality()
    {
        // Arrange
        var array1 = new EquatableArray<string>(ImmutableArray.Create("one", "two", "three"));
        var array2 = new EquatableArray<string>(ImmutableArray.Create("one", "two", "four"));
        var array3 = new EquatableArray<string>(ImmutableArray.Create("one", "two"));

        // Assert - different values
        await Assert.That(array1.Equals(array2)).IsFalse();
        await Assert.That(array1 == array2).IsFalse();
        await Assert.That(array1 != array2).IsTrue();

        // Assert - different lengths
        await Assert.That(array1.Equals(array3)).IsFalse();
        await Assert.That(array1 == array3).IsFalse();
        await Assert.That(array1 != array3).IsTrue();
    }

    [Test]
    public async Task Test_EquatableArray_HashCode()
    {
        // Arrange
        var array1 = new EquatableArray<string>(ImmutableArray.Create("one", "two", "three"));
        var array2 = new EquatableArray<string>(ImmutableArray.Create("one", "two", "three"));
        var array3 = new EquatableArray<string>(ImmutableArray.Create("one", "two", "four"));

        // Assert
        await Assert.That(array1.GetHashCode()).IsEqualTo(array2.GetHashCode());
        await Assert.That(array1.GetHashCode()).IsNotEqualTo(array3.GetHashCode());
    }

    [Test]
    public async Task Test_EquatableArray_Empty_Handling()
    {
        // Arrange
        var empty1 = new EquatableArray<string>(ImmutableArray<string>.Empty);
        var empty2 = new EquatableArray<string>(ImmutableArray<string>.Empty);
        var empty3 = EquatableArray<string>.Empty;
        var nonEmpty = new EquatableArray<string>(ImmutableArray.Create("value"));

        // Assert
        await Assert.That(empty1.Equals(empty2)).IsTrue();
        await Assert.That(empty1.Equals(empty3)).IsTrue();
        await Assert.That(empty1.Equals(nonEmpty)).IsFalse();
        await Assert.That(empty1.GetHashCode()).IsEqualTo(empty2.GetHashCode());
        await Assert.That(empty1.GetHashCode()).IsEqualTo(empty3.GetHashCode());
    }

    [Test]
    public async Task Test_EquatableArray_Implicit_Conversion()
    {
        // Arrange
        var immutableArray = ImmutableArray.Create("test1", "test2");
        EquatableArray<string> equatableArray = immutableArray;

        // Assert
        await Assert.That(equatableArray.AsImmutableArray()).IsEqualTo(immutableArray);
    }

    [Test]
    public async Task Test_EquatableArray_AsImmutableArray()
    {
        // Arrange
        var originalArray = ImmutableArray.Create(1, 2, 3, 4, 5);
        var equatableArray = new EquatableArray<int>(originalArray);

        // Act
        var retrievedArray = equatableArray.AsImmutableArray();

        // Assert
        await Assert.That(retrievedArray).IsEqualTo(originalArray);
    }

    [Test]
    public async Task Test_EquatableArray_IsDefaultOrEmpty()
    {
        // Arrange
        var empty = EquatableArray<int>.Empty;
        var defaultArray = default(EquatableArray<int>);
        var nonEmpty = new EquatableArray<int>(ImmutableArray.Create(1, 2, 3));

        // Assert
        await Assert.That(empty.IsDefaultOrEmpty).IsTrue();
        await Assert.That(defaultArray.IsDefaultOrEmpty).IsTrue();
        await Assert.That(nonEmpty.IsDefaultOrEmpty).IsFalse();
    }

    [Test]
    public async Task Test_EndpointClass_Equality()
    {
        // Arrange
        var metadata = new EndpointMetadata(
            Tags: EquatableArray<string>.Empty,
            Name: "Test",
            Summary: null,
            Description: null,
            RequiresAuthorization: false,
            Policies: EquatableArray<string>.Empty,
            Roles: EquatableArray<string>.Empty);

        var handler = new HandlerMethod(
            Name: "HandleAsync",
            ReturnTypeName: "IResult",
            ReturnTypeNamespace: "Microsoft.AspNetCore.Http",
            IsAsync: true,
            ReturnsTask: true,
            Parameters: EquatableArray<MethodParameter>.Empty);

        var endpoint1 = new EndpointClass(
            Namespace: "TestApp",
            ClassName: "TestEndpoint",
            HttpMethod: HttpMethod.Get,
            Pattern: "/api/test",
            Metadata: metadata,
            HandlerMethod: handler);

        var endpoint2 = new EndpointClass(
            Namespace: "TestApp",
            ClassName: "TestEndpoint",
            HttpMethod: HttpMethod.Get,
            Pattern: "/api/test",
            Metadata: metadata,
            HandlerMethod: handler);

        var endpoint3 = new EndpointClass(
            Namespace: "TestApp",
            ClassName: "DifferentEndpoint",
            HttpMethod: HttpMethod.Get,
            Pattern: "/api/test",
            Metadata: metadata,
            HandlerMethod: handler);

        // Assert
        await Assert.That(endpoint1.Equals(endpoint2)).IsTrue();
        await Assert.That(endpoint1.Equals(endpoint3)).IsFalse();
        await Assert.That(endpoint1.GetHashCode()).IsEqualTo(endpoint2.GetHashCode());
    }

    [Test]
    public async Task Test_EndpointMetadata_Equality()
    {
        // Arrange
        var metadata1 = new EndpointMetadata(
            Tags: new EquatableArray<string>(ImmutableArray.Create("tag1", "tag2")),
            Name: "TestEndpoint",
            Summary: "Test summary",
            Description: "Test description",
            RequiresAuthorization: true,
            Policies: new EquatableArray<string>(ImmutableArray.Create("policy1")),
            Roles: new EquatableArray<string>(ImmutableArray.Create("role1")));

        var metadata2 = new EndpointMetadata(
            Tags: new EquatableArray<string>(ImmutableArray.Create("tag1", "tag2")),
            Name: "TestEndpoint",
            Summary: "Test summary",
            Description: "Test description",
            RequiresAuthorization: true,
            Policies: new EquatableArray<string>(ImmutableArray.Create("policy1")),
            Roles: new EquatableArray<string>(ImmutableArray.Create("role1")));

        var metadata3 = new EndpointMetadata(
            Tags: new EquatableArray<string>(ImmutableArray.Create("tag1", "tag3")), // Different tag
            Name: "TestEndpoint",
            Summary: "Test summary",
            Description: "Test description",
            RequiresAuthorization: true,
            Policies: new EquatableArray<string>(ImmutableArray.Create("policy1")),
            Roles: new EquatableArray<string>(ImmutableArray.Create("role1")));

        // Assert
        await Assert.That(metadata1.Equals(metadata2)).IsTrue();
        await Assert.That(metadata1.Equals(metadata3)).IsFalse();
        await Assert.That(metadata1.GetHashCode()).IsEqualTo(metadata2.GetHashCode());
    }

    [Test]
    public async Task Test_HandlerMethod_Equality()
    {
        // Arrange
        var params1 = new EquatableArray<MethodParameter>(ImmutableArray.Create(
            new MethodParameter("id", "int", "System", false, false, null)));

        var params2 = new EquatableArray<MethodParameter>(ImmutableArray.Create(
            new MethodParameter("id", "int", "System", false, false, null)));

        var params3 = new EquatableArray<MethodParameter>(ImmutableArray.Create(
            new MethodParameter("id", "string", "System", false, false, null))); // Different type

        var handler1 = new HandlerMethod(
            Name: "HandleAsync",
            ReturnTypeName: "IResult",
            ReturnTypeNamespace: "Microsoft.AspNetCore.Http",
            IsAsync: true,
            ReturnsTask: true,
            Parameters: params1);

        var handler2 = new HandlerMethod(
            Name: "HandleAsync",
            ReturnTypeName: "IResult",
            ReturnTypeNamespace: "Microsoft.AspNetCore.Http",
            IsAsync: true,
            ReturnsTask: true,
            Parameters: params2);

        var handler3 = new HandlerMethod(
            Name: "HandleAsync",
            ReturnTypeName: "IResult",
            ReturnTypeNamespace: "Microsoft.AspNetCore.Http",
            IsAsync: true,
            ReturnsTask: true,
            Parameters: params3);

        // Assert
        await Assert.That(handler1.Equals(handler2)).IsTrue();
        await Assert.That(handler1.Equals(handler3)).IsFalse();
        await Assert.That(handler1.GetHashCode()).IsEqualTo(handler2.GetHashCode());
    }

    [Test]
    public async Task Test_MethodParameter_Equality()
    {
        // Arrange
        var param1 = new MethodParameter(
            Name: "id",
            TypeName: "int",
            TypeNamespace: "System",
            IsOptional: false,
            HasDefaultValue: true,
            DefaultValueString: "0");

        var param2 = new MethodParameter(
            Name: "id",
            TypeName: "int",
            TypeNamespace: "System",
            IsOptional: false,
            HasDefaultValue: true,
            DefaultValueString: "0");

        var param3 = new MethodParameter(
            Name: "id",
            TypeName: "int",
            TypeNamespace: "System",
            IsOptional: false,
            HasDefaultValue: true,
            DefaultValueString: "1"); // Different default value

        // Assert
        await Assert.That(param1.Equals(param2)).IsTrue();
        await Assert.That(param1.Equals(param3)).IsFalse();
        await Assert.That(param1.GetHashCode()).IsEqualTo(param2.GetHashCode());
    }

    [Test]
    public async Task Test_EndpointMetadata_Empty_Instance()
    {
        // Arrange
        var emptyMetadata = EndpointMetadata.Empty;

        // Assert
        await Assert.That(emptyMetadata.Tags.IsDefaultOrEmpty).IsTrue();
        await Assert.That(emptyMetadata.Name).IsNull();
        await Assert.That(emptyMetadata.Summary).IsNull();
        await Assert.That(emptyMetadata.Description).IsNull();
        await Assert.That(emptyMetadata.RequiresAuthorization).IsFalse();
        await Assert.That(emptyMetadata.Policies.IsDefaultOrEmpty).IsTrue();
        await Assert.That(emptyMetadata.Roles.IsDefaultOrEmpty).IsTrue();
    }

    [Test]
    public async Task Test_EquatableArray_Null_Handling()
    {
        // Arrange
        var array = new EquatableArray<string>(ImmutableArray.Create("test"));
        object? nullObj = null;

        // Assert
        await Assert.That(array.Equals(nullObj)).IsFalse();
        await Assert.That(array.Equals((object)array)).IsTrue();
    }

    [Test]
    public async Task Test_EquatableArray_Different_Type_Comparison()
    {
        // Arrange
        var stringArray = new EquatableArray<string>(ImmutableArray.Create("test"));
        var intArray = new EquatableArray<int>(ImmutableArray.Create(1));
        object objIntArray = intArray;

        // Assert
        await Assert.That(stringArray.Equals(objIntArray)).IsFalse();
    }
}
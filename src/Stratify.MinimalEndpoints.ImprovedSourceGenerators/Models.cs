using System;
using System.Collections.Immutable;
using System.Linq;

namespace Stratify.MinimalEndpoints.ImprovedSourceGenerators;

// Immutable value-based models that don't contain ITypeSymbol references

internal enum HttpMethod
{
    Unknown = 0,
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options
}

internal readonly record struct EndpointClass(
    string Namespace,
    string ClassName,
    HttpMethod HttpMethod,
    string Pattern,
    EndpointMetadata Metadata,
    HandlerMethod? HandlerMethod);

internal readonly record struct EndpointMetadata(
    EquatableArray<string> Tags,
    string? Name,
    string? Summary,
    string? Description,
    bool RequiresAuthorization,
    EquatableArray<string> Policies,
    EquatableArray<string> Roles)
{
    public static EndpointMetadata Empty => new(
        EquatableArray<string>.Empty,
        null,
        null,
        null,
        false,
        EquatableArray<string>.Empty,
        EquatableArray<string>.Empty);
}

internal readonly record struct HandlerMethod(
    string Name,
    string? ReturnTypeName,
    string? ReturnTypeNamespace,
    bool IsAsync,
    bool ReturnsTask,
    EquatableArray<MethodParameter> Parameters);

internal readonly record struct MethodParameter(
    string Name,
    string TypeName,
    string? TypeNamespace,
    bool IsOptional,
    bool HasDefaultValue,
    string? DefaultValueString); // Store default value as string representation

// EquatableArray implementation for proper collection equality
internal readonly struct EquatableArray<T> : IEquatable<EquatableArray<T>>
{
    public static readonly EquatableArray<T> Empty = new(ImmutableArray<T>.Empty);

    private readonly ImmutableArray<T> _array;

    public EquatableArray(ImmutableArray<T> array)
    {
        _array = array.IsDefault ? ImmutableArray<T>.Empty : array;
    }

    public EquatableArray(T[] array) : this(ImmutableArray.Create(array))
    {
    }

    public ImmutableArray<T> AsImmutableArray() => _array;

    public bool IsDefaultOrEmpty => _array.IsDefaultOrEmpty;

    public int Length => _array.Length;

    public T this[int index] => _array[index];

    public bool Equals(EquatableArray<T> other)
    {
        return _array.SequenceEqual(other._array);
    }

    public override bool Equals(object? obj)
    {
        return obj is EquatableArray<T> array && Equals(array);
    }

    public override int GetHashCode()
    {
        if (_array.IsDefaultOrEmpty)
            return 0;

        unchecked
        {
            int hash = 17;
            foreach (var item in _array)
            {
                hash = hash * 31 + (item?.GetHashCode() ?? 0);
            }
            return hash;
        }
    }

    public static bool operator ==(EquatableArray<T> left, EquatableArray<T> right)
    {
        return left.Equals(right);
    }

    public static bool operator !=(EquatableArray<T> left, EquatableArray<T> right)
    {
        return !left.Equals(right);
    }

    public static implicit operator EquatableArray<T>(ImmutableArray<T> array)
    {
        return new EquatableArray<T>(array);
    }

    public static implicit operator EquatableArray<T>(T[] array)
    {
        return new EquatableArray<T>(array);
    }
}

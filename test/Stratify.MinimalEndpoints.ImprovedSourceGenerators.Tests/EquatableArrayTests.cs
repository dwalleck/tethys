using System.Collections.Immutable;
using Stratify.MinimalEndpoints.ImprovedSourceGenerators;
using TUnit.Assertions;
using TUnit.Assertions.Extensions;
using TUnit.Core;

namespace Stratify.MinimalEndpoints.ImprovedSourceGenerators.Tests;

/// <summary>
/// Comprehensive tests for EquatableArray<T> to ensure 100% code coverage
/// and proper behavior for all scenarios including edge cases.
/// </summary>
public class EquatableArrayTests
{
    #region Construction Tests

    [Test]
    public async Task Constructor_WithImmutableArray_CreatesCorrectInstance()
    {
        // Arrange
        var immutableArray = ImmutableArray.Create("one", "two", "three");
        
        // Act
        var equatableArray = new EquatableArray<string>(immutableArray);
        
        // Assert
        await Assert.That(equatableArray.Length).IsEqualTo(3);
        await Assert.That(equatableArray.AsImmutableArray()).IsEqualTo(immutableArray);
    }

    [Test]
    public async Task Constructor_WithArray_CreatesCorrectInstance()
    {
        // Arrange
        var array = new[] { 1, 2, 3, 4, 5 };
        
        // Act
        var equatableArray = new EquatableArray<int>(array);
        
        // Assert
        await Assert.That(equatableArray.Length).IsEqualTo(5);
        await Assert.That(equatableArray[0]).IsEqualTo(1);
        await Assert.That(equatableArray[4]).IsEqualTo(5);
    }

    [Test]
    public async Task Constructor_WithDefaultImmutableArray_CreatesEmptyArray()
    {
        // Arrange
        var defaultArray = default(ImmutableArray<string>);
        
        // Act
        var equatableArray = new EquatableArray<string>(defaultArray);
        
        // Assert
        await Assert.That(equatableArray.IsDefaultOrEmpty).IsTrue();
        await Assert.That(equatableArray.Length).IsEqualTo(0);
    }

    [Test]
    public async Task Constructor_WithEmptyArray_CreatesEmptyInstance()
    {
        // Arrange
        var emptyArray = Array.Empty<int>();
        
        // Act
        var equatableArray = new EquatableArray<int>(emptyArray);
        
        // Assert
        await Assert.That(equatableArray.IsDefaultOrEmpty).IsTrue();
        await Assert.That(equatableArray.Length).IsEqualTo(0);
    }

    #endregion

    #region Property Tests

    [Test]
    public async Task Length_ReturnsCorrectValue()
    {
        // Arrange
        var array1 = new EquatableArray<string>(new[] { "a", "b", "c" });
        var array2 = new EquatableArray<string>(Array.Empty<string>());
        var array3 = EquatableArray<string>.Empty;
        
        // Assert
        await Assert.That(array1.Length).IsEqualTo(3);
        await Assert.That(array2.Length).IsEqualTo(0);
        await Assert.That(array3.Length).IsEqualTo(0);
    }

    [Test]
    public async Task Indexer_ReturnsCorrectValues()
    {
        // Arrange
        var array = new EquatableArray<int>(new[] { 10, 20, 30, 40, 50 });
        
        // Assert
        await Assert.That(array[0]).IsEqualTo(10);
        await Assert.That(array[1]).IsEqualTo(20);
        await Assert.That(array[2]).IsEqualTo(30);
        await Assert.That(array[3]).IsEqualTo(40);
        await Assert.That(array[4]).IsEqualTo(50);
    }

    [Test]
    public async Task Indexer_WithReferenceTypes_ReturnsCorrectValues()
    {
        // Arrange
        var obj1 = new TestObject { Value = 1 };
        var obj2 = new TestObject { Value = 2 };
        var array = new EquatableArray<TestObject>(new[] { obj1, obj2, null! });
        
        // Assert
        await Assert.That(array[0]).IsSameReferenceAs(obj1);
        await Assert.That(array[1]).IsSameReferenceAs(obj2);
        await Assert.That(array[2]).IsNull();
    }

    #endregion

    #region Equality Tests

    [Test]
    public async Task Equals_WithEqualArrays_ReturnsTrue()
    {
        // Arrange
        var array1 = new EquatableArray<int>(new[] { 1, 2, 3 });
        var array2 = new EquatableArray<int>(new[] { 1, 2, 3 });
        
        // Assert
        await Assert.That(array1.Equals(array2)).IsTrue();
        await Assert.That(array1 == array2).IsTrue();
        await Assert.That(array1 != array2).IsFalse();
    }

    [Test]
    public async Task Equals_WithDifferentArrays_ReturnsFalse()
    {
        // Arrange
        var array1 = new EquatableArray<int>(new[] { 1, 2, 3 });
        var array2 = new EquatableArray<int>(new[] { 1, 2, 4 });
        var array3 = new EquatableArray<int>(new[] { 1, 2 });
        
        // Assert - different values
        await Assert.That(array1.Equals(array2)).IsFalse();
        await Assert.That(array1 == array2).IsFalse();
        await Assert.That(array1 != array2).IsTrue();
        
        // Assert - different lengths
        await Assert.That(array1.Equals(array3)).IsFalse();
    }

    [Test]
    public async Task Equals_WithNullElements_WorksCorrectly()
    {
        // Arrange
        var array1 = new EquatableArray<string>(new[] { "a", null!, "c" });
        var array2 = new EquatableArray<string>(new[] { "a", null!, "c" });
        var array3 = new EquatableArray<string>(new[] { "a", "b", "c" });
        
        // Assert
        await Assert.That(array1.Equals(array2)).IsTrue();
        await Assert.That(array1.Equals(array3)).IsFalse();
    }

    [Test]
    public async Task Equals_WithReferenceTypes_UsesReferenceEquality()
    {
        // Arrange
        var obj1 = new TestObject { Value = 1 };
        var obj2 = new TestObject { Value = 1 }; // Same value but different reference
        var array1 = new EquatableArray<TestObject>(new[] { obj1 });
        var array2 = new EquatableArray<TestObject>(new[] { obj1 });
        var array3 = new EquatableArray<TestObject>(new[] { obj2 });
        
        // Assert
        await Assert.That(array1.Equals(array2)).IsTrue();
        await Assert.That(array1.Equals(array3)).IsFalse(); // Different references
    }

    [Test]
    public async Task Equals_Object_WithNull_ReturnsFalse()
    {
        // Arrange
        var array = new EquatableArray<int>(new[] { 1, 2, 3 });
        
        // Assert
        await Assert.That(array.Equals(null)).IsFalse();
    }

    [Test]
    public async Task Equals_Object_WithSelf_ReturnsTrue()
    {
        // Arrange
        var array = new EquatableArray<int>(new[] { 1, 2, 3 });
        object objArray = array;
        
        // Assert
        await Assert.That(array.Equals(objArray)).IsTrue();
    }

    [Test]
    public async Task Equals_Object_WithDifferentType_ReturnsFalse()
    {
        // Arrange
        var array = new EquatableArray<int>(new[] { 1, 2, 3 });
        object str = "not an array";
        
        // Assert
        await Assert.That(array.Equals(str)).IsFalse();
    }

    [Test]
    public async Task Equals_WithDefaultInstances_WorksCorrectly()
    {
        // Arrange
        var default1 = default(EquatableArray<int>);
        var default2 = default(EquatableArray<int>);
        var empty = EquatableArray<int>.Empty;
        var nonEmpty = new EquatableArray<int>(new[] { 1, 2, 3 });
        
        // Assert - two defaults are equal
        await Assert.That(default1.Equals(default2)).IsTrue();
        await Assert.That(default1 == default2).IsTrue();
        
        // Assert - default is not equal to empty (because empty has non-default array)
        await Assert.That(default1.Equals(empty)).IsFalse();
        await Assert.That(default1 == empty).IsFalse();
        
        // Assert - default is not equal to non-empty
        await Assert.That(default1.Equals(nonEmpty)).IsFalse();
        await Assert.That(default1 == nonEmpty).IsFalse();
    }

    #endregion

    #region Hash Code Tests

    [Test]
    public async Task GetHashCode_EqualArrays_ReturnSameHashCode()
    {
        // Arrange
        var array1 = new EquatableArray<string>(new[] { "one", "two", "three" });
        var array2 = new EquatableArray<string>(new[] { "one", "two", "three" });
        
        // Assert
        await Assert.That(array1.GetHashCode()).IsEqualTo(array2.GetHashCode());
    }

    [Test]
    public async Task GetHashCode_DifferentArrays_ReturnDifferentHashCodes()
    {
        // Arrange
        var array1 = new EquatableArray<string>(new[] { "one", "two", "three" });
        var array2 = new EquatableArray<string>(new[] { "one", "two", "four" });
        
        // Assert
        await Assert.That(array1.GetHashCode()).IsNotEqualTo(array2.GetHashCode());
    }

    [Test]
    public async Task GetHashCode_EmptyArrays_ReturnZero()
    {
        // Arrange
        var empty1 = new EquatableArray<int>(Array.Empty<int>());
        var empty2 = EquatableArray<int>.Empty;
        var defaultArray = default(EquatableArray<int>);
        
        // Assert
        await Assert.That(empty1.GetHashCode()).IsEqualTo(0);
        await Assert.That(empty2.GetHashCode()).IsEqualTo(0);
        await Assert.That(defaultArray.GetHashCode()).IsEqualTo(0);
    }

    [Test]
    public async Task GetHashCode_WithNullElements_HandlesCorrectly()
    {
        // Arrange
        var array1 = new EquatableArray<string>(new[] { "a", null!, "c" });
        var array2 = new EquatableArray<string>(new[] { "a", null!, "c" });
        var array3 = new EquatableArray<string>(new[] { "a", "b", "c" });
        
        // Assert
        await Assert.That(array1.GetHashCode()).IsEqualTo(array2.GetHashCode());
        await Assert.That(array1.GetHashCode()).IsNotEqualTo(array3.GetHashCode());
    }

    #endregion

    #region Implicit Conversion Tests

    [Test]
    public async Task ImplicitConversion_FromImmutableArray_WorksCorrectly()
    {
        // Arrange
        var immutableArray = ImmutableArray.Create(1, 2, 3);
        
        // Act
        EquatableArray<int> equatableArray = immutableArray;
        
        // Assert
        await Assert.That(equatableArray.Length).IsEqualTo(3);
        await Assert.That(equatableArray.AsImmutableArray()).IsEqualTo(immutableArray);
    }

    [Test]
    public async Task ImplicitConversion_FromArray_WorksCorrectly()
    {
        // Arrange
        var array = new[] { "a", "b", "c" };
        
        // Act
        EquatableArray<string> equatableArray = array;
        
        // Assert
        await Assert.That(equatableArray.Length).IsEqualTo(3);
        await Assert.That(equatableArray[0]).IsEqualTo("a");
        await Assert.That(equatableArray[1]).IsEqualTo("b");
        await Assert.That(equatableArray[2]).IsEqualTo("c");
    }

    [Test]
    public async Task ImplicitConversion_FromNullArray_CreatesDefaultArray()
    {
        // Arrange
        int[]? nullArray = null;
        
        // Act
        EquatableArray<int> equatableArray = nullArray!;
        
        // Assert
        // The array constructor passes null to ImmutableArray.Create which returns an empty array
        await Assert.That(equatableArray.IsDefaultOrEmpty).IsTrue();
        await Assert.That(equatableArray.Length).IsEqualTo(0);
    }

    #endregion

    #region Edge Case Tests

    [Test]
    public async Task Empty_Static_Property_IsCorrect()
    {
        // Arrange
        var empty = EquatableArray<string>.Empty;
        
        // Assert
        await Assert.That(empty.IsDefaultOrEmpty).IsTrue();
        await Assert.That(empty.Length).IsEqualTo(0);
        await Assert.That(empty.GetHashCode()).IsEqualTo(0);
    }

    [Test]
    public async Task Default_Instance_BehavesAsEmpty()
    {
        // Arrange
        var defaultArray = default(EquatableArray<int>);
        
        // Assert
        await Assert.That(defaultArray.IsDefaultOrEmpty).IsTrue();
        await Assert.That(defaultArray.Length).IsEqualTo(0);
        await Assert.That(defaultArray.GetHashCode()).IsEqualTo(0);
    }
    
    [Test]
    public async Task Default_Instance_Indexer_ThrowsException()
    {
        // Arrange
        var defaultArray = default(EquatableArray<int>);
        
        // Act & Assert
        await Assert.ThrowsAsync<InvalidOperationException>(async () =>
        {
            var value = defaultArray[0];
            await Task.CompletedTask;
        });
    }

    [Test]
    public async Task Operators_WorkCorrectly()
    {
        // Arrange
        var array1 = new EquatableArray<int>(new[] { 1, 2, 3 });
        var array2 = new EquatableArray<int>(new[] { 1, 2, 3 });
        var array3 = new EquatableArray<int>(new[] { 1, 2, 4 });
        
        // Assert
        await Assert.That(array1 == array2).IsTrue();
        await Assert.That(array1 != array2).IsFalse();
        await Assert.That(array1 == array3).IsFalse();
        await Assert.That(array1 != array3).IsTrue();
    }

    #endregion

    #region Performance Characteristics Documentation

    [Test]
    [Skip("Performance documentation test - run manually to verify performance characteristics")]
    public async Task DocumentPerformanceCharacteristics()
    {
        /*
         * Performance Characteristics of EquatableArray<T>:
         * 
         * 1. Construction:
         *    - From ImmutableArray<T>: O(1) - just wraps the reference
         *    - From T[]: O(n) - creates ImmutableArray internally
         * 
         * 2. Equality:
         *    - Equals(): O(n) - must compare all elements
         *    - Best case: O(1) if lengths differ
         *    - Worst case: O(n) if arrays are equal or differ at the end
         * 
         * 3. Hash Code:
         *    - GetHashCode(): O(n) - must process all elements
         *    - Empty arrays: O(1) - returns 0 immediately
         * 
         * 4. Property Access:
         *    - Length: O(1)
         *    - IsDefaultOrEmpty: O(1)
         *    - Indexer: O(1)
         *    - AsImmutableArray(): O(1)
         * 
         * 5. Memory:
         *    - Overhead: Minimal - just wraps ImmutableArray<T>
         *    - No additional allocations beyond the underlying array
         * 
         * 6. Thread Safety:
         *    - Fully thread-safe due to immutability
         *    - Can be safely shared across threads without synchronization
         */
        
        await Task.CompletedTask;
    }

    #endregion

    #region Helper Classes

    private class TestObject
    {
        public int Value { get; set; }
    }

    #endregion
}
using System.Collections.Generic;
using System.Collections.Immutable;
using System.Linq;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.Text;

namespace Tethys.MinimalEndpoints.SourceGenerators;

[Generator]
public class EndpointGenerator : IIncrementalGenerator
{
    // Define well-known type names as constants
    private const string EndpointAttributeName = "EndpointAttribute";
    private const string EndpointMetadataAttributeName = "EndpointMetadataAttribute";
    private const string HandlerAttributeName = "HandlerAttribute";

    // Define well-known type full names for robust comparison
    private const string EndpointAttributeFullName = "Tethys.MinimalEndpoints.EndpointAttribute";
    private const string EndpointMetadataAttributeFullName = "Tethys.MinimalEndpoints.EndpointMetadataAttribute";
    private const string HandlerAttributeFullName = "Tethys.MinimalEndpoints.HandlerAttribute";

    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        // Find all classes with [Endpoint] attribute
        var endpointClasses = context.SyntaxProvider
            .CreateSyntaxProvider(
                predicate: static (s, _) => IsPotentialEndpointClass(s),
                transform: static (ctx, _) => GetEndpointClassOrNull(ctx))
            .Where(static m => m is not null);

        // Combine all endpoint classes and generate
        var compilation = context.CompilationProvider.Combine(endpointClasses.Collect());

        context.RegisterSourceOutput(compilation,
            static (spc, source) => Execute(source.Left, source.Right!, spc));
    }

    private static bool IsPotentialEndpointClass(SyntaxNode node)
    {
        return node is ClassDeclarationSyntax c &&
               c.AttributeLists.Count > 0 &&
               c.Modifiers.Any(SyntaxKind.PartialKeyword);
    }

    private static EndpointClass? GetEndpointClassOrNull(GeneratorSyntaxContext context)
    {
        var classDeclaration = (ClassDeclarationSyntax)context.Node;
        var symbol = context.SemanticModel.GetDeclaredSymbol(classDeclaration);

        if (symbol is null)
        {
            return null;
        }

        // Check for [Endpoint] attribute with full type checking
        var endpointAttribute = symbol.GetAttributes()
            .FirstOrDefault(a => IsEndpointAttribute(a.AttributeClass));

        if (endpointAttribute is null)
        {
            return null;
        }

        // Extract strongly-typed HTTP method
        var httpMethod = ExtractHttpMethod(endpointAttribute);
        if (httpMethod == HttpMethod.Unknown)
        {
            return null; // Invalid HTTP method
        }

        // Extract pattern with validation
        var pattern = ExtractPattern(endpointAttribute);
        if (!IsValidPattern(pattern))
        {
            return null; // Invalid pattern
        }

        // Check for [EndpointMetadata] attribute
        var metadataAttribute = symbol.GetAttributes()
            .FirstOrDefault(a => IsEndpointMetadataAttribute(a.AttributeClass));

        var metadata = ExtractMetadata(metadataAttribute);

        // Find handler method with validation
        var handlerMethod = FindHandlerMethod(symbol);

        return new EndpointClass
        {
            Namespace = symbol.ContainingNamespace.ToDisplayString(),
            ClassName = symbol.Name,
            HttpMethod = httpMethod,
            Pattern = pattern,
            Metadata = metadata,
            HandlerMethod = handlerMethod
        };
    }

    private static bool IsEndpointAttribute(INamedTypeSymbol? attributeClass)
    {
        if (attributeClass is null) return false;

        return attributeClass.ToDisplayString() == EndpointAttributeFullName ||
               attributeClass.Name == EndpointAttributeName;
    }

    private static bool IsEndpointMetadataAttribute(INamedTypeSymbol? attributeClass)
    {
        if (attributeClass is null) return false;

        return attributeClass.ToDisplayString() == EndpointMetadataAttributeFullName ||
               attributeClass.Name == EndpointMetadataAttributeName;
    }

    private static bool IsHandlerAttribute(INamedTypeSymbol? attributeClass)
    {
        if (attributeClass is null) return false;

        return attributeClass.ToDisplayString() == HandlerAttributeFullName ||
               attributeClass.Name == HandlerAttributeName;
    }

    private static HttpMethod ExtractHttpMethod(AttributeData attribute)
    {
        if (attribute.ConstructorArguments.Length == 0)
        {
            return HttpMethod.Unknown;
        }

        var methodArg = attribute.ConstructorArguments[0];
        if (methodArg.Kind != TypedConstantKind.Enum || methodArg.Type?.TypeKind != TypeKind.Enum)
        {
            return HttpMethod.Unknown;
        }

        var enumValue = (int)(methodArg.Value ?? 0);
        var enumType = methodArg.Type;

        // Get the enum member name from the value
        var enumMember = enumType.GetMembers()
            .OfType<IFieldSymbol>()
            .FirstOrDefault(f => f.HasConstantValue && (int)f.ConstantValue == enumValue);

        if (enumMember is null)
        {
            return HttpMethod.Unknown;
        }

        // Map enum name to HttpMethod
        return enumMember.Name switch
        {
            "Get" => HttpMethod.Get,
            "Post" => HttpMethod.Post,
            "Put" => HttpMethod.Put,
            "Delete" => HttpMethod.Delete,
            "Patch" => HttpMethod.Patch,
            "Head" => HttpMethod.Head,
            "Options" => HttpMethod.Options,
            _ => HttpMethod.Unknown
        };
    }

    private static string ExtractPattern(AttributeData attribute)
    {
        if (attribute.ConstructorArguments.Length < 2)
        {
            return "/";
        }

        var patternArg = attribute.ConstructorArguments[1];
        return patternArg.Value?.ToString() ?? "/";
    }

    private static bool IsValidPattern(string pattern)
    {
        // Add pattern validation logic
        return !string.IsNullOrWhiteSpace(pattern) && pattern.StartsWith("/");
    }

    private static HandlerMethod? FindHandlerMethod(INamedTypeSymbol classSymbol)
    {
        var method = classSymbol.GetMembers()
            .OfType<IMethodSymbol>()
            .FirstOrDefault(m => m.GetAttributes()
                .Any(a => IsHandlerAttribute(a.AttributeClass)));

        if (method is null)
        {
            return null;
        }

        return new HandlerMethod
        {
            Name = method.Name,
            ReturnType = method.ReturnType,
            Parameters = method.Parameters.Select(p => new MethodParameter
            {
                Name = p.Name,
                Type = p.Type,
                IsOptional = p.IsOptional,
                HasDefaultValue = p.HasExplicitDefaultValue,
                DefaultValue = p.ExplicitDefaultValue
            }).ToImmutableArray()
        };
    }

    private static EndpointMetadata ExtractMetadata(AttributeData? attribute)
    {
        var metadata = new EndpointMetadata();

        if (attribute is null)
        {
            return metadata;
        }

        foreach (var arg in attribute.NamedArguments)
        {
            switch (arg.Key)
            {
                case nameof(EndpointMetadata.Tags):
                    metadata.Tags = ExtractStringArray(arg.Value);
                    break;
                case nameof(EndpointMetadata.Name):
                    metadata.Name = arg.Value.Value?.ToString();
                    break;
                case nameof(EndpointMetadata.Summary):
                    metadata.Summary = arg.Value.Value?.ToString();
                    break;
                case nameof(EndpointMetadata.Description):
                    metadata.Description = arg.Value.Value?.ToString();
                    break;
                case nameof(EndpointMetadata.RequiresAuthorization):
                    metadata.RequiresAuthorization = ExtractBooleanValue(arg.Value);
                    break;
                case nameof(EndpointMetadata.Policies):
                    metadata.Policies = ExtractStringArray(arg.Value);
                    break;
                case nameof(EndpointMetadata.Roles):
                    metadata.Roles = ExtractStringArray(arg.Value);
                    break;
            }
        }

        return metadata;
    }

    private static ImmutableArray<string> ExtractStringArray(TypedConstant constant)
    {
        if (constant.Kind != TypedConstantKind.Array)
        {
            return ImmutableArray<string>.Empty;
        }

        return constant.Values
            .Where(v => v.Value is string)
            .Select(v => (string)v.Value!)
            .ToImmutableArray();
    }

    private static bool ExtractBooleanValue(TypedConstant constant)
    {
        return constant.Kind == TypedConstantKind.Primitive &&
               constant.Value is bool value &&
               value;
    }

    private static void Execute(Compilation compilation, ImmutableArray<EndpointClass?> endpoints, SourceProductionContext context)
    {
        if (endpoints.IsDefaultOrEmpty)
        {
            return;
        }

        var validEndpoints = endpoints.Where(e => e != null).Cast<EndpointClass>().ToList();
        if (!validEndpoints.Any())
        {
            return;
        }

        var source = GenerateEndpointImplementations(validEndpoints);
        context.AddSource("GeneratedEndpoints.g.cs", SourceText.From(source, Encoding.UTF8));
    }

    private static string GenerateEndpointImplementations(List<EndpointClass> endpoints)
    {
        var sb = new StringBuilder();

        sb.AppendLine("// <auto-generated/>");
        sb.AppendLine("using Microsoft.AspNetCore.Builder;");
        sb.AppendLine("using Microsoft.AspNetCore.Http;");
        sb.AppendLine("using Microsoft.AspNetCore.Routing;");
        sb.AppendLine("using Tethys.MinimalEndpoints;");
        sb.AppendLine();

        foreach (var endpoint in endpoints)
        {
            sb.AppendLine($"namespace {endpoint.Namespace}");
            sb.AppendLine("{");
            sb.AppendLine($"    partial class {endpoint.ClassName} : IEndpoint");
            sb.AppendLine("    {");
            sb.AppendLine("        public void MapEndpoint(IEndpointRouteBuilder app)");
            sb.AppendLine("        {");

            // Use strongly-typed HTTP method
            var methodName = GetMapMethodName(endpoint.HttpMethod);
            sb.Append($"            app.{methodName}(\"{endpoint.Pattern}\", ");

            if (endpoint.HandlerMethod != null)
            {
                sb.Append(endpoint.HandlerMethod.Name);
            }
            else
            {
                sb.Append("HandleAsync");
            }

            sb.AppendLine(")");

            // Add metadata with proper null checking
            AppendMetadata(sb, endpoint.Metadata);

            sb.AppendLine("                ;");
            sb.AppendLine("        }");
            sb.AppendLine("    }");
            sb.AppendLine("}");
            sb.AppendLine();
        }

        return sb.ToString();
    }

    private static string GetMapMethodName(HttpMethod method)
    {
        return method switch
        {
            HttpMethod.Get => "MapGet",
            HttpMethod.Post => "MapPost",
            HttpMethod.Put => "MapPut",
            HttpMethod.Delete => "MapDelete",
            HttpMethod.Patch => "MapPatch",
            HttpMethod.Head => "MapMethods",
            HttpMethod.Options => "MapMethods",
            _ => throw new InvalidOperationException($"Unsupported HTTP method: {method}")
        };
    }

    private static void AppendMetadata(StringBuilder sb, EndpointMetadata metadata)
    {
        if (!metadata.Tags.IsDefaultOrEmpty)
        {
            var tags = string.Join(", ", metadata.Tags.Select(t => $"\"{EscapeString(t)}\""));
            sb.AppendLine($"                .WithTags({tags})");
        }

        if (!string.IsNullOrEmpty(metadata.Name))
        {
            sb.AppendLine($"                .WithName(\"{EscapeString(metadata.Name)}\")");
        }

        if (!string.IsNullOrEmpty(metadata.Summary))
        {
            sb.AppendLine($"                .WithSummary(\"{EscapeString(metadata.Summary)}\")");
        }

        if (!string.IsNullOrEmpty(metadata.Description))
        {
            sb.AppendLine($"                .WithDescription(\"{EscapeString(metadata.Description)}\")");
        }

        if (metadata.RequiresAuthorization)
        {
            sb.AppendLine("                .RequireAuthorization()");

            if (!metadata.Policies.IsDefaultOrEmpty)
            {
                foreach (var policy in metadata.Policies)
                {
                    sb.AppendLine($"                .RequireAuthorization(\"{EscapeString(policy)}\")");
                }
            }

            if (!metadata.Roles.IsDefaultOrEmpty)
            {
                var roles = string.Join(", ", metadata.Roles.Select(r => $"\"{EscapeString(r)}\""));
                sb.AppendLine($"                .RequireAuthorization(policy => policy.RequireRole({roles}))");
            }
        }
    }

    private static string EscapeString(string value)
    {
        return value.Replace("\"", "\\\"").Replace("\n", "\\n").Replace("\r", "\\r");
    }

    // Strongly-typed models
    private enum HttpMethod
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

    private sealed class EndpointClass
    {
        public required string Namespace { get; init; }
        public required string ClassName { get; init; }
        public required HttpMethod HttpMethod { get; init; }
        public required string Pattern { get; init; }
        public required EndpointMetadata Metadata { get; init; }
        public HandlerMethod? HandlerMethod { get; init; }
    }

    private sealed class EndpointMetadata
    {
        public ImmutableArray<string> Tags { get; set; } = ImmutableArray<string>.Empty;
        public string? Name { get; set; }
        public string? Summary { get; set; }
        public string? Description { get; set; }
        public bool RequiresAuthorization { get; set; }
        public ImmutableArray<string> Policies { get; set; } = ImmutableArray<string>.Empty;
        public ImmutableArray<string> Roles { get; set; } = ImmutableArray<string>.Empty;
    }

    private sealed class HandlerMethod
    {
        public required string Name { get; init; }
        public required ITypeSymbol ReturnType { get; init; }
        public required ImmutableArray<MethodParameter> Parameters { get; init; }
    }

    private sealed class MethodParameter
    {
        public required string Name { get; init; }
        public required ITypeSymbol Type { get; init; }
        public required bool IsOptional { get; init; }
        public required bool HasDefaultValue { get; init; }
        public object? DefaultValue { get; init; }
    }
}

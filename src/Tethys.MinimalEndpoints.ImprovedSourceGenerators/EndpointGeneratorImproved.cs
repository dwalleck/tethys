using System;
using System.Collections.Generic;
using System.Collections.Immutable;
using System.Linq;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.Text;

namespace Tethys.MinimalEndpoints.ImprovedSourceGenerators;

[Generator]
public class EndpointGeneratorImproved : IIncrementalGenerator
{
    // Define well-known type full names for robust comparison
    private const string EndpointAttributeFullName = "Tethys.MinimalEndpoints.Attributes.EndpointAttribute";
    private const string EndpointMetadataAttributeFullName = "Tethys.MinimalEndpoints.Attributes.EndpointMetadataAttribute";
    private const string HandlerAttributeFullName = "Tethys.MinimalEndpoints.Attributes.HandlerAttribute";

    // Tracking names for debugging
    private static class TrackingNames
    {
        public const string EndpointExtraction = nameof(EndpointExtraction);
        public const string EndpointGeneration = nameof(EndpointGeneration);
    }

    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        // Find all classes with [Endpoint] attribute using the new efficient API
        var endpointClasses = context.SyntaxProvider
            .ForAttributeWithMetadataName(
                EndpointAttributeFullName,
                predicate: static (node, _) => node is ClassDeclarationSyntax c &&
                    c.Modifiers.Any(SyntaxKind.PartialKeyword) &&
                    !c.Modifiers.Any(SyntaxKind.AbstractKeyword),
                transform: static (ctx, _) => GetEndpointClassOrNull(ctx))
            .Where(static m => m.HasValue)
            .Select(static (m, _) => m!.Value)
            .WithTrackingName(TrackingNames.EndpointExtraction);

        // Generate source code for each endpoint
        context.RegisterSourceOutput(
            endpointClasses.Collect(),
            static (spc, endpoints) => Execute(endpoints, spc));
    }

    private static EndpointClass? GetEndpointClassOrNull(GeneratorAttributeSyntaxContext context)
    {
        var classDeclaration = (ClassDeclarationSyntax)context.TargetNode;
        var symbol = context.TargetSymbol as INamedTypeSymbol;

        if (symbol is null)
        {
            return null;
        }

        // Get the [Endpoint] attribute from the context
        var endpointAttribute = context.Attributes.FirstOrDefault(a => 
            a.AttributeClass?.ToDisplayString() == EndpointAttributeFullName);

        if (endpointAttribute is null)
        {
            return null;
        }

        // Extract strongly-typed HTTP method
        var httpMethod = ExtractHttpMethod(endpointAttribute);
        if (httpMethod == HttpMethod.Unknown)
        {
            // TODO: Report diagnostic
            return null;
        }

        // Extract pattern with validation
        var pattern = ExtractPattern(endpointAttribute);
        if (!IsValidPattern(pattern))
        {
            // TODO: Report diagnostic
            return null;
        }

        // Check for [EndpointMetadata] attribute
        var metadataAttribute = symbol.GetAttributes()
            .FirstOrDefault(a => a.AttributeClass?.ToDisplayString() == EndpointMetadataAttributeFullName);

        var metadata = ExtractMetadata(metadataAttribute);

        // Find handler method with validation
        var handlerMethod = FindHandlerMethod(symbol);

        return new EndpointClass(
            Namespace: symbol.ContainingNamespace.ToDisplayString(),
            ClassName: symbol.Name,
            HttpMethod: httpMethod,
            Pattern: pattern,
            Metadata: metadata,
            HandlerMethod: handlerMethod);
    }

    private static HttpMethod ExtractHttpMethod(AttributeData attribute)
    {
        if (attribute.ConstructorArguments.Length < 2)
        {
            return HttpMethod.Unknown;
        }

        var methodArg = attribute.ConstructorArguments[1];
        if (methodArg.Kind != TypedConstantKind.Enum || methodArg.Type?.TypeKind != TypeKind.Enum)
        {
            return HttpMethod.Unknown;
        }

        // Get the underlying type of the enum
        var enumType = methodArg.Type as INamedTypeSymbol;
        if (enumType is null)
        {
            return HttpMethod.Unknown;
        }
        
        // Convert the value to int64 to handle all integral types
        long enumValueAsLong;
        
        try
        {
            if (methodArg.Value is null)
            {
                enumValueAsLong = 0;
            }
            else
            {
                // Use Convert.ToInt64 which handles all integral types
                enumValueAsLong = Convert.ToInt64(methodArg.Value);
            }
        }
        catch
        {
            return HttpMethod.Unknown;
        }

        // Find the enum member with matching value
        var enumMember = enumType.GetMembers()
            .OfType<IFieldSymbol>()
            .FirstOrDefault(f => f.HasConstantValue && 
                           Convert.ToInt64(f.ConstantValue) == enumValueAsLong);

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
        if (attribute.ConstructorArguments.Length == 0)
        {
            return "/";
        }

        var patternArg = attribute.ConstructorArguments[0];
        return patternArg.Value?.ToString() ?? "/";
    }

    private static bool IsValidPattern(string pattern)
    {
        // Add pattern validation logic
        return !string.IsNullOrWhiteSpace(pattern) && pattern.StartsWith("/");
    }

    private static HandlerMethod? FindHandlerMethod(INamedTypeSymbol classSymbol)
    {
        // First, try to find a method with [Handler] attribute
        var method = classSymbol.GetMembers()
            .OfType<IMethodSymbol>()
            .FirstOrDefault(m => m.GetAttributes()
                .Any(a => a.AttributeClass?.ToDisplayString() == HandlerAttributeFullName));

        // If no [Handler] attribute, look for Handle or HandleAsync method
        if (method is null)
        {
            method = classSymbol.GetMembers()
                .OfType<IMethodSymbol>()
                .FirstOrDefault(m => m.Name == "Handle" || m.Name == "HandleAsync");
        }

        if (method is null)
        {
            return null;
        }

        // Extract type information as strings instead of storing ITypeSymbol
        string? returnTypeName = null;
        string? returnTypeNamespace = null;
        bool isAsync = false;
        bool returnsTask = false;

        if (method.ReturnType is not null)
        {
            returnTypeName = method.ReturnType.Name;
            returnTypeNamespace = method.ReturnType.ContainingNamespace?.ToDisplayString();
            
            // Check if it's async/Task
            var returnTypeFullName = method.ReturnType.ToDisplayString();
            isAsync = method.IsAsync;
            returnsTask = returnTypeFullName.StartsWith("System.Threading.Tasks.Task");
        }

        var parameters = method.Parameters.Select(p => new MethodParameter(
            Name: p.Name,
            TypeName: p.Type.Name,
            TypeNamespace: p.Type.ContainingNamespace?.ToDisplayString(),
            IsOptional: p.IsOptional,
            HasDefaultValue: p.HasExplicitDefaultValue,
            DefaultValueString: p.HasExplicitDefaultValue ? GetDefaultValueString(p.ExplicitDefaultValue) : null
        )).ToImmutableArray();

        return new HandlerMethod(
            Name: method.Name,
            ReturnTypeName: returnTypeName,
            ReturnTypeNamespace: returnTypeNamespace,
            IsAsync: isAsync,
            ReturnsTask: returnsTask,
            Parameters: new EquatableArray<MethodParameter>(parameters));
    }

    private static string? GetDefaultValueString(object? defaultValue)
    {
        return defaultValue switch
        {
            null => "null",
            string s => $"\"{s}\"",
            char c => $"'{c}'",
            bool b => b ? "true" : "false",
            _ => defaultValue.ToString()
        };
    }

    private static EndpointMetadata ExtractMetadata(AttributeData? attribute)
    {
        if (attribute is null)
        {
            return EndpointMetadata.Empty;
        }

        var tags = EquatableArray<string>.Empty;
        string? name = null;
        string? summary = null;
        string? description = null;
        bool requiresAuthorization = false;
        var policies = EquatableArray<string>.Empty;
        var roles = EquatableArray<string>.Empty;

        foreach (var arg in attribute.NamedArguments)
        {
            switch (arg.Key)
            {
                case "Tags":
                    tags = ExtractStringArray(arg.Value);
                    break;
                case "Name":
                    name = arg.Value.Value?.ToString();
                    break;
                case "Summary":
                    summary = arg.Value.Value?.ToString();
                    break;
                case "Description":
                    description = arg.Value.Value?.ToString();
                    break;
                case "RequiresAuthorization":
                    requiresAuthorization = ExtractBooleanValue(arg.Value);
                    break;
                case "Policies":
                    policies = ExtractStringArray(arg.Value);
                    break;
                case "Roles":
                    roles = ExtractStringArray(arg.Value);
                    break;
            }
        }

        return new EndpointMetadata(
            Tags: tags,
            Name: name,
            Summary: summary,
            Description: description,
            RequiresAuthorization: requiresAuthorization,
            Policies: policies,
            Roles: roles);
    }

    private static EquatableArray<string> ExtractStringArray(TypedConstant constant)
    {
        if (constant.Kind != TypedConstantKind.Array)
        {
            return EquatableArray<string>.Empty;
        }

        var values = constant.Values
            .Where(v => v.Value is string)
            .Select(v => (string)v.Value!)
            .ToImmutableArray();

        return new EquatableArray<string>(values);
    }

    private static bool ExtractBooleanValue(TypedConstant constant)
    {
        return constant.Kind == TypedConstantKind.Primitive &&
               constant.Value is bool value &&
               value;
    }

    private static void Execute(ImmutableArray<EndpointClass> endpoints, SourceProductionContext context)
    {
        if (endpoints.IsDefaultOrEmpty)
        {
            return;
        }

        var source = GenerateEndpointImplementations(endpoints);
        context.AddSource("GeneratedEndpoints.g.cs", SourceText.From(source, Encoding.UTF8));
    }

    private static string GenerateEndpointImplementations(ImmutableArray<EndpointClass> endpoints)
    {
        var sb = new StringBuilder();

        sb.AppendLine("// <auto-generated/>");
        sb.AppendLine("using System.Collections.Generic;");
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

            if (endpoint.HandlerMethod.HasValue)
            {
                sb.Append(endpoint.HandlerMethod.Value.Name);
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
            HttpMethod.Unknown => "MapGet", // Default to GET for safety
            _ => "MapGet" // Default fallback
        };
    }

    private static void AppendMetadata(StringBuilder sb, EndpointMetadata metadata)
    {
        if (!metadata.Tags.IsDefaultOrEmpty)
        {
            var tags = string.Join(", ", metadata.Tags.AsImmutableArray().Select(t => $"\"{EscapeString(t)}\""));
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
                foreach (var policy in metadata.Policies.AsImmutableArray())
                {
                    sb.AppendLine($"                .RequireAuthorization(\"{EscapeString(policy)}\")");
                }
            }

            if (!metadata.Roles.IsDefaultOrEmpty)
            {
                var roles = string.Join(", ", metadata.Roles.AsImmutableArray().Select(r => $"\"{EscapeString(r)}\""));
                sb.AppendLine($"                .RequireAuthorization(policy => policy.RequireRole({roles}))");
            }
        }
    }

    private static string EscapeString(string value)
    {
        return value.Replace("\"", "\\\"").Replace("\n", "\\n").Replace("\r", "\\r");
    }
}
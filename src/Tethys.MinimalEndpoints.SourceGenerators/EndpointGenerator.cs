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

        // Check for [Endpoint] attribute
        var endpointAttribute = symbol.GetAttributes()
            .FirstOrDefault(a => a.AttributeClass?.Name == "EndpointAttribute");

        if (endpointAttribute is null)
        {
            return null;
        }

        // Extract attribute data
        // The enum value comes through as an integer, but we can get the type and field name
        var methodEnumValue = (int)(endpointAttribute.ConstructorArguments[0].Value ?? 0);
        var enumType = endpointAttribute.ConstructorArguments[0].Type;

        // Get the enum member name from the value
        var methodName = "Get"; // Default
        if (enumType != null && enumType.TypeKind == TypeKind.Enum)
        {
            var enumMembers = enumType.GetMembers().OfType<IFieldSymbol>()
                .Where(f => f.HasConstantValue && (int)f.ConstantValue == methodEnumValue);
            var enumMember = enumMembers.FirstOrDefault();
            if (enumMember != null)
            {
                methodName = enumMember.Name;
            }
        }

        var pattern = endpointAttribute.ConstructorArguments[1].Value?.ToString() ?? "/";

        // Check for [EndpointMetadata] attribute
        var metadataAttribute = symbol.GetAttributes()
            .FirstOrDefault(a => a.AttributeClass?.Name == "EndpointMetadataAttribute");

        var metadata = ExtractMetadata(metadataAttribute);

        // Find handler method
        var handlerMethod = symbol.GetMembers()
            .OfType<IMethodSymbol>()
            .FirstOrDefault(m => m.GetAttributes()
                .Any(a => a.AttributeClass?.Name == "HandlerAttribute"));

        return new EndpointClass
        {
            Namespace = symbol.ContainingNamespace.ToDisplayString(),
            ClassName = symbol.Name,
            HttpMethod = methodName,
            Pattern = pattern,
            Metadata = metadata,
            HandlerMethod = handlerMethod
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
                case "Tags":
                    metadata.Tags = arg.Value.Values
                        .Select(v => v.Value?.ToString())
                        .Where(v => v != null)
                        .ToArray()!;
                    break;
                case "Name":
                    metadata.Name = arg.Value.Value?.ToString();
                    break;
                case "Summary":
                    metadata.Summary = arg.Value.Value?.ToString();
                    break;
                case "Description":
                    metadata.Description = arg.Value.Value?.ToString();
                    break;
                case "RequiresAuthorization":
                    metadata.RequiresAuthorization = (bool)(arg.Value.Value ?? false);
                    break;
                case "Policies":
                    metadata.Policies = arg.Value.Values
                        .Select(v => v.Value?.ToString())
                        .Where(v => v != null)
                        .ToArray()!;
                    break;
                case "Roles":
                    metadata.Roles = arg.Value.Values
                        .Select(v => v.Value?.ToString())
                        .Where(v => v != null)
                        .ToArray()!;
                    break;
            }
        }

        return metadata;
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

            // Convert enum name to ASP.NET Core method name (e.g., "Get" -> "MapGet")
            sb.Append($"            app.Map{endpoint.HttpMethod}(\"{endpoint.Pattern}\", ");

            if (endpoint.HandlerMethod != null)
            {
                sb.Append(endpoint.HandlerMethod.Name);
            }
            else
            {
                sb.Append("HandleAsync");
            }

            sb.AppendLine(")");

            // Add metadata
            if (endpoint.Metadata.Tags?.Length > 0)
            {
                var tags = string.Join(", ", endpoint.Metadata.Tags.Select(t => $"\"{t}\""));
                sb.AppendLine($"                .WithTags({tags})");
            }

            if (!string.IsNullOrEmpty(endpoint.Metadata.Name))
            {
                sb.AppendLine($"                .WithName(\"{endpoint.Metadata.Name}\")");
            }

            if (!string.IsNullOrEmpty(endpoint.Metadata.Summary))
            {
                sb.AppendLine($"                .WithSummary(\"{endpoint.Metadata.Summary}\")");
            }

            if (!string.IsNullOrEmpty(endpoint.Metadata.Description))
            {
                sb.AppendLine($"                .WithDescription(\"{endpoint.Metadata.Description}\")");
            }

            if (endpoint.Metadata.RequiresAuthorization)
            {
                sb.AppendLine("                .RequireAuthorization()");

                if (endpoint.Metadata.Policies?.Length > 0)
                {
                    foreach (var policy in endpoint.Metadata.Policies)
                    {
                        sb.AppendLine($"                .RequireAuthorization(\"{policy}\")");
                    }
                }

                if (endpoint.Metadata.Roles?.Length > 0)
                {
                    var roles = string.Join(", ", endpoint.Metadata.Roles.Select(r => $"\"{r}\""));
                    sb.AppendLine($"                .RequireAuthorization(policy => policy.RequireRole({roles}))");
                }
            }

            sb.AppendLine("                ;");
            sb.AppendLine("        }");
            sb.AppendLine("    }");
            sb.AppendLine("}");
            sb.AppendLine();
        }

        return sb.ToString();
    }

    private class EndpointClass
    {
        public string Namespace { get; set; } = "";
        public string ClassName { get; set; } = "";
        public string HttpMethod { get; set; } = "Get";
        public string Pattern { get; set; } = "";
        public EndpointMetadata Metadata { get; set; } = new();
        public IMethodSymbol? HandlerMethod { get; set; }
    }

    private class EndpointMetadata
    {
        public string[]? Tags { get; set; }
        public string? Name { get; set; }
        public string? Summary { get; set; }
        public string? Description { get; set; }
        public bool RequiresAuthorization { get; set; }
        public string[]? Policies { get; set; }
        public string[]? Roles { get; set; }
    }
}

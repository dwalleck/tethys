using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Http;

namespace Stratify.MinimalEndpoints;

internal static class RouteHandlerBuilderExtensions
{
    public static RouteHandlerBuilder WithTagsIfAny(this RouteHandlerBuilder builder, string[]? tags)
    {
        return tags?.Length > 0 ? builder.WithTags(tags) : builder;
    }

    public static RouteHandlerBuilder WithNameIfProvided(this RouteHandlerBuilder builder, string? name)
    {
        return !string.IsNullOrWhiteSpace(name) ? builder.WithName(name) : builder;
    }

    public static RouteHandlerBuilder WithSummaryIfProvided(this RouteHandlerBuilder builder, string? summary)
    {
        return !string.IsNullOrWhiteSpace(summary) ? builder.WithSummary(summary) : builder;
    }

    public static RouteHandlerBuilder WithDescriptionIfProvided(this RouteHandlerBuilder builder, string? description)
    {
        return !string.IsNullOrWhiteSpace(description) ? builder.WithDescription(description) : builder;
    }
}

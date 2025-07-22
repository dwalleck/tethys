namespace Stratify.Api.Features.Environments;

public class Environment
{
    public Guid Id { get; set; }
    public string Name { get; set; } = string.Empty;
    public string Description { get; set; } = string.Empty;
}

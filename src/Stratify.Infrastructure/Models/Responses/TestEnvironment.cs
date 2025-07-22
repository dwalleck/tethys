using System.ComponentModel.DataAnnotations;

namespace Stratify.Infrastructure.Models.Responses;

public class TestEnvironment
{
    public Guid Id { get; set; }
    [Required]
    public string Name { get; set; } = string.Empty;
}

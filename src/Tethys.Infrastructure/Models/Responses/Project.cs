using System.ComponentModel.DataAnnotations;

namespace Tethys.Infrastructure.Models.Responses;
public class Project
{
    public Guid Id { get; set; }
    [Required]
    public string Name { get; set; } = string.Empty;
    public string Description { get; set; } = string.Empty;
}

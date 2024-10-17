using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Tethys.Data.Models.Responses;

namespace Tethys.Data.Services;
public interface IProjectService
{
    Task<List<Project>> GetProjectsAsync();
    Task<Project?> GetProjectAsync(Guid id);
    Task<Project> CreateProjectAsync(Project project);
    Task<Project?> UpdateProjectAsync(Guid id, Project project);
    Task<Project?> DeleteProjectAsync(Guid id);
}

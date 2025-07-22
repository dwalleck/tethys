// using Microsoft.EntityFrameworkCore;
// using Stratify.Infrastructure.Models.Responses;

// namespace Stratify.Infrastructure.Services;
// public class ProjectService(StratifyContext dbContext) : IProjectService
// {
//     private readonly StratifyContext _dbContext = dbContext;

//     public Task<List<Project>> GetProjectsAsync()
//     {
//         return _dbContext.Projects.ToListAsync();
//     }

//     public Task<Project?> GetProjectAsync(Guid id)
//     {
//         return _dbContext.Projects.FirstOrDefaultAsync(p => p.Id == id);
//     }

//     public async Task<Project> CreateProjectAsync(Project project)
//     {
//         project.Id = Guid.NewGuid();
//         _dbContext.Projects.Add(project);
//         await _dbContext.SaveChangesAsync().ConfigureAwait(false);
//         return project;
//     }

//     public async Task<Project?> UpdateProjectAsync(Guid id, Project project)
//     {
//         var existingProject = await _dbContext.Projects.FirstOrDefaultAsync(p => p.Id == id).ConfigureAwait(false);
//         if (existingProject is null)
//         {
//             return null;
//         }
//         existingProject.Name = project.Name;
//         existingProject.Description = project.Description;
//         await _dbContext.SaveChangesAsync().ConfigureAwait(false);
//         return existingProject;
//     }

//     public async Task<Project?> DeleteProjectAsync(Guid id)
//     {
//         var existingProject = await _dbContext.Projects.FirstOrDefaultAsync(p => p.Id == id).ConfigureAwait(false);
//         if (existingProject is null)
//         {
//             return null;
//         }
//         _dbContext.Projects.Remove(existingProject);
//         await _dbContext.SaveChangesAsync().ConfigureAwait(false);
//         return existingProject;
//     }
// }


using Microsoft.EntityFrameworkCore;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Tethys.Data.Models.Responses;

namespace Tethys.Data.Services;
public class ProjectService : IProjectService
{
    private readonly TethysContext _dbContext;

    public ProjectService(TethysContext dbContext)
    {
        _dbContext = dbContext;
    }

    public Task<List<Project>> GetProjectsAsync()
    {
        return _dbContext.Projects.ToListAsync();
    }

    public Task<Project?> GetProjectAsync(Guid id)
    {
        return _dbContext.Projects.FirstOrDefaultAsync(p => p.Id == id);
    }

    public async Task<Project> CreateProjectAsync(Project project)
    {
        project.Id = Guid.NewGuid();
        _dbContext.Projects.Add(project);
        await _dbContext.SaveChangesAsync();
        return project;
    }

    public async Task<Project?> UpdateProjectAsync(Guid id, Project project)
    {
        var existingProject = await _dbContext.Projects.FirstOrDefaultAsync(p => p.Id == id);
        if (existingProject is null)
        {
            return null;
        }
        existingProject.Name = project.Name;
        existingProject.Description = project.Description;
        await _dbContext.SaveChangesAsync();
        return existingProject;
    }

    public async Task<Project?> DeleteProjectAsync(Guid id)
    {
        var existingProject = await _dbContext.Projects.FirstOrDefaultAsync(p => p.Id == id);
        if (existingProject is null)
        {
            return null;
        }
        _dbContext.Projects.Remove(existingProject);
        await _dbContext.SaveChangesAsync();
        return existingProject;
    }
}


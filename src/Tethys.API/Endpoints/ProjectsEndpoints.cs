using Microsoft.AspNetCore.Http;
using Microsoft.AspNetCore.Http.HttpResults;
using Microsoft.AspNetCore.Mvc.ModelBinding;
using Microsoft.EntityFrameworkCore;
using Tethys.Data.Models.Requests;
using Tethys.Data.Models.Responses;
using Tethys.Data.Services;

namespace Tethys.API.Endpoints;

public static class ProjectsEndpoints
{
    public static void RegisterProjectsEndpoints(this WebApplication app)
    {
        app.MapGet("/projects", async (IProjectService projectService) =>
        {
            var projects = await projectService.GetProjectsAsync();
            return TypedResults.Ok(projects);
        });

        app.MapGet("/projects/{id}", async Task<Results<Ok<Project>, NotFound>> (IProjectService projectService, Guid id) =>
        {
            var project = await projectService.GetProjectAsync(id);
            if (project is null)
            {
                return TypedResults.NotFound();
            }
            return TypedResults.Ok(project);
        });

        //TODO: Figure out why the return type here causes errors Task<Results<Created<Project>>>
        app.MapPost("/projects", async  (IProjectService projectService, CreateProjectRequest request) =>
        {
            var project = new Project
            {
                Name = request.Name,
                Description = request.Description
            };
            var createdProject = await projectService.CreateProjectAsync(project);
            return TypedResults.Created($"/projects/{createdProject.Id}", createdProject);
        });

        app.MapPut("/projects/{id}", async Task<Results<Ok<Project>, NotFound>> (IProjectService projectService, Guid id, Project project) =>
        {
            var updatedProject = await projectService.UpdateProjectAsync(id, project);
            if (updatedProject is null)
            {
                return TypedResults.NotFound();
            }
            return TypedResults.Ok(updatedProject);
        });

        app.MapDelete("/projects/{id}", async Task<Results<Ok<Project>, NotFound>> (IProjectService projectService, Guid id) =>
        {
            var deletedProject = await projectService.DeleteProjectAsync(id);
            if (deletedProject is null)
            {
                return TypedResults.NotFound();
            }
            return TypedResults.Ok(deletedProject);
        });
    }
}


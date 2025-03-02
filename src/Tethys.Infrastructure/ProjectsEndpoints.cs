// using Microsoft.AspNetCore.Http.HttpResults;
// using Tethys.Infrastructure.Models.Requests;
// using Tethys.Infrastructure.Models.Responses;
// using Tethys.Infrastructure.Services;

// namespace Tethys.Api.Features.Projects;

// public static class ProjectsEndpoints
// {
//     public static void RegisterProjectsEndpoints(this WebApplication app)
//     {
//         var projects = app.MapGroup("/projects");
//         projects.MapGet("/", GetProjectsAsync);
//         projects.MapGet("/{id}", GetProjectAsync);
//         projects.MapPost("/", CreateProjectAsync);
//         projects.MapPut("/{id}", UpdateProjectAsync);
//         projects.MapDelete("/{id}", DeleteProjectAsync);
//     }

//     public static async Task<IResult> GetProjectsAsync(IProjectService projectService)
//     {
//         var projects = await projectService.GetProjectsAsync().ConfigureAwait(false);
//         return TypedResults.Ok(projects);
//     }

//     public static async Task<Results<Ok<Project>, NotFound>> GetProjectAsync(IProjectService projectService, Guid id)
//     {
//         var project = await projectService.GetProjectAsync(id).ConfigureAwait(false);
//         if (project is null)
//         {
//             return TypedResults.NotFound();
//         }
//         return TypedResults.Ok(project);
//     }

//     public static async Task<IResult> CreateProjectAsync(IProjectService projectService, CreateProjectRequest request)
//     {
//         var project = new Project
//         {
//             Name = request.Name,
//             Description = request.Description
//         };
//         var createdProject = await projectService.CreateProjectAsync(project).ConfigureAwait(false);
//         return TypedResults.Created($"/projects/{createdProject.Id}");
//     }

//     public static async Task<Results<Ok<Project>, NotFound>> UpdateProjectAsync(IProjectService projectService, Guid id, Project project)
//     {
//         var updatedProject = await projectService.UpdateProjectAsync(id, project).ConfigureAwait(false);
//         if (updatedProject is null)
//         {
//             return TypedResults.NotFound();
//         }
//         return TypedResults.Ok(updatedProject);
//     }

//     public static async Task<Results<Ok<Project>, NotFound>> DeleteProjectAsync(IProjectService projectService, Guid id)
//     {
//         var deletedProject = await projectService.DeleteProjectAsync(id).ConfigureAwait(false);
//         if (deletedProject is null)
//         {
//             return TypedResults.NotFound();
//         }
//         return TypedResults.Ok(deletedProject);
//     }

// }


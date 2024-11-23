using Microsoft.AspNetCore.Http.HttpResults;
using Moq;
using Tethys.API.Endpoints;
using Tethys.Infrastructure.Models.Responses;
using Tethys.Infrastructure.Models.Requests;
using Tethys.Infrastructure.Services;
using Xunit;

namespace Tethys.API.Tests.Endpoints
{
    public class ProjectsEndpointsTest
    {
        [Fact]
        public async Task GetProjectsAsync_ReturnsOkResult_WithListOfProjects()
        {
            // Arrange
            var mockProjectService = new Mock<IProjectService>();
            var projects = new List<Project>
            {
                new Project { Id = Guid.NewGuid(), Name = "Project 1", Description = "Description 1" },
                new Project { Id = Guid.NewGuid(), Name = "Project 2", Description = "Description 2" }
            };
            mockProjectService.Setup(service => service.GetProjectsAsync()).ReturnsAsync(projects);

            // Act
            var result = await ProjectsEndpoints.GetProjectsAsync(mockProjectService.Object);

            // Assert
            var okResult = Assert.IsType<Ok<List<Project>>>(result);
            Assert.Equal(projects, okResult.Value);
        }

        [Fact]
        public async Task GetProjectAsync_ReturnsOkResult_WhenProjectExists()
        {
            // Arrange
            var mockProjectService = new Mock<IProjectService>();
            var projectId = Guid.NewGuid();
            var project = new Project { Id = projectId, Name = "Project 1", Description = "Description 1" };
            mockProjectService.Setup(service => service.GetProjectAsync(projectId)).ReturnsAsync(project);

            // Act
            var result = await ProjectsEndpoints.GetProjectAsync(mockProjectService.Object, projectId);

            // Assert
            var okResult = Assert.IsType<Ok<Project>>(result.Result);
            Assert.Equal(project, okResult.Value);
        }

        [Fact]
        public async Task GetProjectAsync_ReturnsNotFound_WhenProjectDoesNotExist()
        {
            // Arrange
            var mockProjectService = new Mock<IProjectService>();
            var projectId = Guid.NewGuid();
            mockProjectService.Setup(service => service.GetProjectAsync(projectId)).ReturnsAsync((Project)null);

            // Act
            var result = await ProjectsEndpoints.GetProjectAsync(mockProjectService.Object, projectId);

            // Assert
            Assert.IsType<NotFound>(result.Result);
        }

        [Fact]
        public async Task CreateProjectAsync_ReturnsCreatedResult()
        {
            // Arrange
            var mockProjectService = new Mock<IProjectService>();
            var request = new CreateProjectRequest { Name = "New Project", Description = "New Description" };
            var project = new Project { Id = Guid.NewGuid(), Name = request.Name, Description = request.Description };
            mockProjectService.Setup(service => service.CreateProjectAsync(It.IsAny<Project>())).ReturnsAsync(project);

            // Act
            var result = await ProjectsEndpoints.CreateProjectAsync(mockProjectService.Object, request);

            // Assert
            var createdResult = Assert.IsType<Created<Project>>(result);
            Assert.Equal($"/projects/{project.Id}", createdResult.Location);
        }

        [Fact]
        public async Task UpdateProjectAsync_ReturnsOkResult_WhenProjectIsUpdated()
        {
            // Arrange
            var mockProjectService = new Mock<IProjectService>();
            var projectId = Guid.NewGuid();
            var project = new Project { Id = projectId, Name = "Updated Project", Description = "Updated Description" };
            mockProjectService.Setup(service => service.UpdateProjectAsync(projectId, project)).ReturnsAsync(project);

            // Act
            var result = await ProjectsEndpoints.UpdateProjectAsync(mockProjectService.Object, projectId, project);

            // Assert
            var okResult = Assert.IsType<Ok<Project>>(result.Result);
            Assert.Equal(project, okResult.Value);
        }

        [Fact]
        public async Task UpdateProjectAsync_ReturnsNotFound_WhenProjectDoesNotExist()
        {
            // Arrange
            var mockProjectService = new Mock<IProjectService>();
            var projectId = Guid.NewGuid();
            var project = new Project { Id = projectId, Name = "Updated Project", Description = "Updated Description" };
            mockProjectService.Setup(service => service.UpdateProjectAsync(projectId, project)).ReturnsAsync((Project)null);

            // Act
            var result = await ProjectsEndpoints.UpdateProjectAsync(mockProjectService.Object, projectId, project);

            // Assert
            Assert.IsType<NotFound>(result.Result);
        }

        [Fact]
        public async Task DeleteProjectAsync_ReturnsOkResult_WhenProjectIsDeleted()
        {
            // Arrange
            var mockProjectService = new Mock<IProjectService>();
            var projectId = Guid.NewGuid();
            var project = new Project { Id = projectId, Name = "Project to Delete", Description = "Description" };
            mockProjectService.Setup(service => service.DeleteProjectAsync(projectId)).ReturnsAsync(project);

            // Act
            var result = await ProjectsEndpoints.DeleteProjectAsync(mockProjectService.Object, projectId);

            // Assert
            var okResult = Assert.IsType<Ok<Project>>(result.Result);
            Assert.Equal(project, okResult.Value);
        }

        [Fact]
        public async Task DeleteProjectAsync_ReturnsNotFound_WhenProjectDoesNotExist()
        {
            // Arrange
            var mockProjectService = new Mock<IProjectService>();
            var projectId = Guid.NewGuid();
            mockProjectService.Setup(service => service.DeleteProjectAsync(projectId)).ReturnsAsync((Project)null);

            // Act
            var result = await ProjectsEndpoints.DeleteProjectAsync(mockProjectService.Object, projectId);

            // Assert
            Assert.IsType<NotFound>(result.Result);
        }
    }
}


























































































































































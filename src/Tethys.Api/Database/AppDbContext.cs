using Microsoft.EntityFrameworkCore;
using Tethys.Infrastructure.Models.Responses;
using Tethys.Api.Features.Projects;

namespace Tethys.Api.Database;

public class AppDbContext(DbContextOptions<AppDbContext> options) : DbContext(options)
{
    public DbSet<Project> Projects => Set<Project>();
    public DbSet<TestEnvironment> TestEnvironments => Set<TestEnvironment>();
}


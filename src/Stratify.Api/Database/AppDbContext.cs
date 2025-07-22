using Microsoft.EntityFrameworkCore;
using Stratify.Infrastructure.Models.Responses;
using Stratify.Api.Features.Projects;

namespace Stratify.Api.Database;

public class AppDbContext(DbContextOptions<AppDbContext> options) : DbContext(options)
{
    public DbSet<Project> Projects => Set<Project>();
    public DbSet<TestEnvironment> TestEnvironments => Set<TestEnvironment>();
}


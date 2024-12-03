using Microsoft.EntityFrameworkCore;
using Tethys.Infrastructure.Models.Responses;

namespace Tethys.Infrastructure;

public class TethysContext : DbContext
{
    public TethysContext(DbContextOptions<TethysContext> options) : base(options) { }

    public DbSet<Project> Projects => Set<Project>();
    public DbSet<TestEnvironment> TestEnvironments => Set<TestEnvironment>();
}


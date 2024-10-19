using Microsoft.EntityFrameworkCore;
using System.Collections.Generic;
using Tethys.Data.Models.Responses;

namespace Tethys.Data;










public class TethysContext : DbContext
{
    public TethysContext(DbContextOptions<TethysContext> options)
    : base(options)
    {
    }

    public DbSet<Project> Projects { get; set; }
    public DbSet<TestEnvironment> Environments { get; set; }
}


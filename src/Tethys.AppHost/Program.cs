var builder = DistributedApplication.CreateBuilder(args);

builder.AddProject<Projects.Tethys_API>("tethys-api");

builder.Build().Run();

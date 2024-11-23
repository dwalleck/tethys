var builder = DistributedApplication.CreateBuilder(args);

builder.AddProject<Projects.Tethys_Api>("tethys-api");

builder.Build().Run();

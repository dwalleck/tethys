var builder = DistributedApplication.CreateBuilder(args);

builder.AddProject<Projects.Stratify_Api>("Stratify-api");

builder.Build().Run();

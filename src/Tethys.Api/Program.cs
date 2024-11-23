using Microsoft.EntityFrameworkCore;
using Tethys.Api.Endpoints;
using Tethys.Infrastructure;
using Tethys.Infrastructure.Services;

var builder = WebApplication.CreateBuilder(args);

builder.AddServiceDefaults();
builder.Services.AddCors();

builder.Services.AddDbContext<TethysContext>(options =>
{
    options.UseInMemoryDatabase("Tethys");
});

builder.Services.AddScoped<IProjectService, ProjectService>();

// Add services to the container.
// Learn more about configuring OpenAPI at https://aka.ms/aspnet/openapi
builder.Services.AddOpenApi();


var app = builder.Build();
app.UseCors();
app.MapDefaultEndpoints();

// Configure the HTTP request pipeline.
if (app.Environment.IsDevelopment())
{
    app.MapOpenApi();
    app.UseSwaggerUI(options =>
    {
        options.SwaggerEndpoint("/openapi/v1.json", "v1");
    });
    app.UseCors(options => options
        .AllowAnyOrigin()
        .AllowAnyMethod()
        .AllowAnyHeader());
}

app.UseHttpsRedirection();
app.RegisterProjectsEndpoints();

app.Run();
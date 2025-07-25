using FluentValidation;
using Microsoft.EntityFrameworkCore;
using Stratify.Api.Database;
using Stratify.MinimalEndpoints;

var builder = WebApplication.CreateBuilder(args);

builder.AddServiceDefaults();
builder.Services.AddEndpointsApiExplorer();
builder.Services.AddEndpoints();

builder.Services.AddCors();

builder.Services.AddDbContext<AppDbContext>(options =>
{
    options.UseInMemoryDatabase("Stratify");
});

builder.Services.AddValidatorsFromAssembly(typeof(Program).Assembly);

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
        //options.CustomSchemaIds(t => t.FullName?.Replace('+', '.'));
        options.SwaggerEndpoint("/openapi/v1.json", "v1");
    });
    app.UseCors(options => options
        .AllowAnyOrigin()
        .AllowAnyMethod()
        .AllowAnyHeader());
}

app.UseHttpsRedirection();
app.MapEndpoints();

app.Run();

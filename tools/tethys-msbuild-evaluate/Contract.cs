using System;
using System.Collections.Generic;
using Newtonsoft.Json;

namespace Tethys.MSBuild.Evaluate;

internal sealed class Request
{
    [JsonProperty(Required = Required.Always)] public int protocol_version { get; set; }
    [JsonProperty(Required = Required.Always)] public string workspace_root { get; set; } = "";
    [JsonProperty(Required = Required.Always)] public string project_path { get; set; } = "";
    [JsonProperty(Required = Required.AllowNull)] public string? target_framework { get; set; }
    [JsonProperty(Required = Required.Always)] public Dictionary<string, string> global_properties { get; set; } = new(StringComparer.OrdinalIgnoreCase);
    [JsonProperty(Required = Required.Always)] public string msbuild_path { get; set; } = "";
    [JsonProperty(Required = Required.Always)] public bool trust_granted { get; set; }
}

internal sealed class Response
{
    public int protocol_version { get; set; } = 1;
    public bool success { get; set; }
    public string project_path { get; set; } = "";
    public Host? host { get; set; }
    public Dictionary<string, string> properties { get; set; } = new(StringComparer.OrdinalIgnoreCase);
    public Dictionary<string, List<Item>> items { get; set; } = new()
    {
        ["Compile"] = new(), ["ProjectReference"] = new(), ["Reference"] = new()
    };
    public List<string> imports { get; set; } = new();
    public List<GlobPattern> glob_patterns { get; set; } = new();
    public List<Diagnostic> diagnostics { get; set; } = new();
    public bool cache_eligible { get; set; }
    public List<string> cache_ineligibility { get; set; } = new();
}

internal sealed class Host
{
    public string kind { get; set; } = "";
    public string path { get; set; } = "";
    public string version { get; set; } = "";
    public string runtime { get; set; } = "";
}

internal sealed class Item
{
    public string include { get; set; } = "";
    public string full_path { get; set; } = "";
    public Dictionary<string, string> metadata { get; set; } = new(StringComparer.OrdinalIgnoreCase);
}

internal sealed class GlobPattern
{
    public string project_path { get; set; } = "";
    public string item_type { get; set; } = "";
    public string include { get; set; } = "";
    public string exclude { get; set; } = "";
    public string remove { get; set; } = "";
}

internal sealed class Diagnostic
{
    public string? code { get; set; }
    public string message { get; set; } = "";
    public string? file { get; set; }
    public int line { get; set; }
    public int column { get; set; }
    public string severity { get; set; } = "error";
    public string? exception_type { get; set; }
}

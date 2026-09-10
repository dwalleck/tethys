using System.Diagnostics;
using Microsoft.Build.Locator;
using Newtonsoft.Json;

namespace Tethys.MSBuild.Evaluate;

internal static class PerformanceProbe
{
    private static int Main(string[] args)
    {
        var started = Stopwatch.StartNew();
        var requests = JsonConvert.DeserializeObject<List<Request>>(File.ReadAllText(args[0]))
            ?? throw new InvalidDataException("Missing requests");
        if (requests.Count == 0 || requests.Any(request => !request.trust_granted || request.msbuild_path != requests[0].msbuild_path))
            throw new InvalidDataException("Probe requires one explicitly trusted host");
        MSBuildLocator.RegisterMSBuildPath(requests[0].msbuild_path);
        if (args.Length > 2 && args[2] == "park")
        {
            if (requests.Count != 1) throw new InvalidDataException("A parked worker is one-shot");
            WarmHost(requests[0]);
            Console.WriteLine("READY");
            if (Console.ReadLine() != "GO") throw new InvalidDataException("Missing evaluation release");
        }
        var setupSeconds = started.Elapsed.TotalSeconds;
        var rows = new List<object>();
        var success = true;
        foreach (var request in requests)
        {
            var evaluation = Stopwatch.StartNew();
            var response = Evaluation.Run(request);
            var evaluationSeconds = evaluation.Elapsed.TotalSeconds;
            var serialization = Stopwatch.StartNew();
            var json = JsonConvert.SerializeObject(response);
            var serializationSeconds = serialization.Elapsed.TotalSeconds;
            rows.Add(new { evaluation_seconds = evaluationSeconds, serialization_seconds = serializationSeconds, response_json = json });
            success &= response.success;
        }
        File.WriteAllText(args[1], JsonConvert.SerializeObject(new { setup_seconds = setupSeconds, body_seconds = started.Elapsed.TotalSeconds, calls = rows }, Formatting.Indented));
        return success ? 0 : 1;
    }

    [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.NoInlining)]
    private static void WarmHost(Request request)
    {
        // No project XML, SDK resolver or build target is invoked by this probe.
        using var collection = new Microsoft.Build.Evaluation.ProjectCollection(request.global_properties);
        collection.IsBuildEnabled = false;
        var response = new Response();
        response.items["Compile"].Add(new Item());
        _ = JsonConvert.SerializeObject(response);
    }
}

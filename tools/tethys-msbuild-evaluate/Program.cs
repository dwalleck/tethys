using System;
using System.IO;
using System.Text;
using Microsoft.Build.Locator;
using Newtonsoft.Json;

namespace Tethys.MSBuild.Evaluate;

internal static class Program
{
    private static int Main()
    {
        var serializer = JsonSerializer.Create(new JsonSerializerSettings { MaxDepth = 64, CheckAdditionalContent = true });
        var response = new Response();
        using var stderr = new LimitedStream(Console.OpenStandardError(), 1024 * 1024);
        Console.SetError(new StreamWriter(stderr, new UTF8Encoding(false)) { AutoFlush = true });
        Console.SetOut(Console.Error);
        try
        {
            using var input = Console.OpenStandardInput();
            using var bytes = new MemoryStream();
            var chunk = new byte[8192];
            int count;
            while ((count = input.Read(chunk, 0, chunk.Length)) != 0)
            {
                if (bytes.Length + count > 1024 * 1024) throw new IOException("Request exceeds 1 MiB.");
                bytes.Write(chunk, 0, count);
            }
            bytes.Position = 0;
            using var text = new StreamReader(bytes, new UTF8Encoding(false, true), false);
            using var json = new JsonTextReader(text);
            var request = serializer.Deserialize<Request>(json) ?? throw new JsonSerializationException("Request is null.");
            response.project_path = request.project_path;
            if (request.protocol_version != 1) throw new ArgumentException("Unsupported protocol version.");
            if (!request.trust_granted) throw new UnauthorizedAccessException("MSBuild evaluation requires explicit trust.");
            foreach (var path in new[] { request.workspace_root, request.project_path, request.msbuild_path })
                if (string.IsNullOrWhiteSpace(path) || !Path.IsPathRooted(path) || Path.GetFullPath(path) != path)
                    throw new ArgumentException("Request paths must be normalized absolute paths.");
            if (!Directory.Exists(request.workspace_root)) throw new DirectoryNotFoundException(request.workspace_root);
            if (!File.Exists(Path.Combine(request.msbuild_path, "Microsoft.Build.dll")))
                throw new FileNotFoundException("Selected MSBuild installation is unavailable.", request.msbuild_path);
            MSBuildLocator.RegisterMSBuildPath(request.msbuild_path);
            response = Evaluation.Run(request);
            if (stderr.Overflowed) throw new IOException("Diagnostic stream exceeds 1 MiB.");
        }
        catch (Exception error) { response = Failure(response, error); }
        using var output = new MemoryStream();
        try { Serialize(response, output, serializer); }
        catch (Exception error)
        {
            response = Failure(response, error);
            output.SetLength(0);
            Serialize(response, output, serializer);
        }
        output.Position = 0;
        output.CopyTo(Console.OpenStandardOutput());
        return response.success ? 0 : 1;
    }
    private static Response Failure(Response prior, Exception error) => new()
    {
        project_path = prior.project_path, host = prior.host,
        cache_ineligibility = new() { "evaluation_failed" },
        diagnostics = new() { new Diagnostic { exception_type = error.GetType().FullName,
            message = error.Message.Length <= 8192 ? error.Message : error.Message.Substring(0, 8192) } }
    };
    private static void Serialize(Response response, Stream output, JsonSerializer serializer)
    {
        using var bounded = new LimitedStream(output, 64 * 1024 * 1024);
        using var text = new StreamWriter(bounded, new UTF8Encoding(false), 8192, true);
        using var json = new JsonTextWriter(text);
        serializer.Serialize(json, response);
    }
    private sealed class LimitedStream(Stream inner, long limit) : Stream
    {
        private long written;
        public bool Overflowed { get; private set; }
        public override void Write(byte[] buffer, int offset, int count)
        {
            if (count > limit - written) { Overflowed = true; throw new IOException("Protocol stream byte limit exceeded."); }
            written += count;
            inner.Write(buffer, offset, count);
        }
        public override bool CanRead => false;
        public override bool CanSeek => false;
        public override bool CanWrite => true;
        public override long Length => written;
        public override long Position { get => written; set => throw new NotSupportedException(); }
        public override void Flush() => inner.Flush();
        public override int Read(byte[] buffer, int offset, int count) => throw new NotSupportedException();
        public override long Seek(long offset, SeekOrigin origin) => throw new NotSupportedException();
        public override void SetLength(long value) => throw new NotSupportedException();
    }
}

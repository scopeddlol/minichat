using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json;
using MiniChat.Native;

static void Check(bool condition, string detail) { if (!condition) throw new Exception(detail); }
foreach (var address in new[] { "http://example.com", "file:///tmp/chat", "https://user:password@example.com", "https://example.com/path", "https://example.com/?token=secret" })
{
    try { using var invalid = new MiniChatApi(address); throw new Exception("Accepted invalid origin: " + address); }
    catch (ArgumentException) { }
}
var portPicker = new TcpListener(IPAddress.Loopback, 0);
portPicker.Start();
var port = ((IPEndPoint)portPicker.LocalEndpoint).Port;
portPicker.Stop();
using var listener = new HttpListener();
var origin = $"http://localhost:{port}/";
listener.Prefixes.Add(origin);
listener.Start();
var server = Task.Run(async () => {
    for (var i = 0; i < 5; i++)
    {
        var context = await listener.GetContextAsync().WaitAsync(TimeSpan.FromSeconds(10));
        var req = context.Request;
        string json;
        if (i == 0)
        {
            Check(req.RawUrl == "/api/auth/login" && req.HttpMethod == "POST", "Login request");
            using var doc = await JsonDocument.ParseAsync(req.InputStream);
            Check(doc.RootElement.GetProperty("username").GetString() == "alice", "Username body");
            json = "{\"token\":\"test-token\"}";
        }
        else
        {
            Check(req.Headers["Authorization"] == "Bearer test-token", "Missing bearer token");
            json = i switch {
                1 => "[{\"id\":\"general\",\"name\":\"General\",\"kind\":\"text\",\"position\":0}]",
                2 => "[{\"id\":\"1\",\"content\":\"Hello\",\"author\":{\"display_name\":\"Alice\"}}]",
                3 => "{\"id\":\"2\",\"content\":\"Reply\",\"author\":{\"display_name\":\"Alice\"}}",
                _ => "{\"error\":\"Expired\"}"
            };
            if (i == 1) Check(req.RawUrl == "/api/channels", "Channel request");
            if (i == 2) Check(req.RawUrl == "/api/channels/general/messages?limit=100", "History request");
            if (i == 3)
            {
                Check(req.HttpMethod == "POST" && req.RawUrl == "/api/channels/general/messages", "Send request");
                using var doc = await JsonDocument.ParseAsync(req.InputStream);
                Check(doc.RootElement.GetProperty("content").GetString() == "Reply", "Message body");
            }
            if (i == 4) context.Response.StatusCode = 401;
        }
        context.Response.ContentType = "application/json";
        var bytes = Encoding.UTF8.GetBytes(json);
        await context.Response.OutputStream.WriteAsync(bytes);
        context.Response.Close();
    }
});
using var api = new MiniChatApi(origin);
await api.Login("alice", "password");
Check((await api.Channels()).Single().id == "general", "Channel response");
Check((await api.Messages("general")).Single().AuthorName == "Alice", "Message response");
Check((await api.Send("general", "Reply")).content == "Reply", "Send response");
try { await api.Channels(); throw new Exception("Expired session accepted"); }
catch (SessionExpiredException) { }
await server;
Console.WriteLine("Native API smoke passed: origins, login, bearer auth, channels, history, send, session expiry.");

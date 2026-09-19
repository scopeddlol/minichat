using System.Net;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text.Json;

namespace MiniChat.Native;

// No web renderer or browser runtime. Authentication lives only in memory.
public sealed class MiniChatApi : IDisposable
{
    private readonly HttpClient http;
    public MiniChatApi(string address)
    {
        if (!Uri.TryCreate(address.Trim(), UriKind.Absolute, out var uri)
            || (uri.Scheme != "https" && !(uri.Scheme == "http" && uri.IsLoopback))
            || uri.UserInfo.Length != 0 || uri.AbsolutePath != "/" || uri.Query.Length != 0 || uri.Fragment.Length != 0)
            throw new ArgumentException("Enter an HTTPS instance origin, such as https://chat.example.com. HTTP is allowed only on localhost.");
        http = new HttpClient(new HttpClientHandler { AllowAutoRedirect = false })
        {
            BaseAddress = uri,
            Timeout = TimeSpan.FromSeconds(20)
        };
    }
    public async Task Login(string username, string password)
    {
        var auth = await Request<Auth>(HttpMethod.Post, "api/auth/login", new { username, password });
        http.DefaultRequestHeaders.Authorization = new AuthenticationHeaderValue("Bearer", auth.token);
    }
    public Task<List<Channel>> Channels() => Request<List<Channel>>(HttpMethod.Get, "api/channels");
    public Task<List<Message>> Messages(string id) => Request<List<Message>>(HttpMethod.Get, $"api/channels/{Uri.EscapeDataString(id)}/messages?limit=100");
    public Task<Message> Send(string id, string content) => Request<Message>(HttpMethod.Post, $"api/channels/{Uri.EscapeDataString(id)}/messages", new { content });
    private async Task<T> Request<T>(HttpMethod method, string path, object? body = null)
    {
        using var request = new HttpRequestMessage(method, path);
        if (body != null) request.Content = JsonContent.Create(body);
        using var response = await http.SendAsync(request);
        if (!response.IsSuccessStatusCode)
        {
            if (response.StatusCode == HttpStatusCode.Unauthorized) throw new SessionExpiredException();
            var detail = $"Request failed ({(int)response.StatusCode}).";
            try { detail = (await response.Content.ReadFromJsonAsync<ApiError>())?.error ?? detail; }
            catch (JsonException) { }
            throw new HttpRequestException(detail);
        }
        return await response.Content.ReadFromJsonAsync<T>() ?? throw new HttpRequestException("The instance returned an empty response.");
    }
    public void Dispose() => http.Dispose();
    private sealed record Auth(string token);
    private sealed record ApiError(string error);
}
public sealed class SessionExpiredException : Exception
{
    public SessionExpiredException() : base("Sign in again. Your credentials or session were rejected.") { }
}
public sealed record Channel(string id, string name, string kind, int position);
public sealed record Author(string display_name);
public sealed record Attachment(string filename);
public sealed record Message(string id, string content, Author? author, string? webhook_name, List<Attachment>? attachments)
{
    public string AuthorName => author?.display_name ?? webhook_name ?? "Deleted member";
    public string AttachmentNames => string.Join(", ", attachments?.Select(a => a.filename) ?? []);
}

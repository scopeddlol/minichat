using System.Windows;
using System.Windows.Controls;
using System.Windows.Threading;

namespace MiniChat.Native;

public partial class MainWindow : Window
{
    private MiniChatApi? api;
    private int revision;
    private bool refreshing;
    private readonly DispatcherTimer timer = new() { Interval = TimeSpan.FromSeconds(3) };
    public MainWindow()
    {
        InitializeComponent();
        timer.Tick += async (_, _) => await Refresh();
        Closed += (_, _) => { timer.Stop(); api?.Dispose(); };
    }
    private async void Login(object sender, RoutedEventArgs e)
    {
        LoginButton.IsEnabled = false;
        MiniChatApi? candidate = null;
        try
        {
            candidate = new MiniChatApi(Instance.Text);
            await candidate.Login(Username.Text.Trim(), Password.Password);
            var channels = await candidate.Channels();
            api = candidate;
            candidate = null;
            Password.Clear();
            LoginPanel.Visibility = Visibility.Collapsed;
            ChatPanel.Visibility = Visibility.Visible;
            LoginButton.IsDefault = false;
            Channels.ItemsSource = channels.Where(c => c.kind != "voice").OrderBy(c => c.position).ToList();
            Channels.SelectedIndex = Channels.Items.Count > 0 ? 0 : -1;
            timer.Start();
            Status.Text = "Signed in. Latest 100 messages refresh every 3 seconds.";
        }
        catch (Exception error) { Status.Text = error.Message; }
        finally { candidate?.Dispose(); LoginButton.IsEnabled = true; }
    }
    private void Logout(object sender, RoutedEventArgs e) => SignOut();
    private void SignOut()
    {
        ++revision;
        timer.Stop();
        api?.Dispose();
        api = null;
        Channels.ItemsSource = null;
        Messages.ItemsSource = null;
        Draft.Clear();
        SendButton.IsEnabled = false;
        ChatPanel.Visibility = Visibility.Collapsed;
        LoginPanel.Visibility = Visibility.Visible;
        LoginButton.IsDefault = true;
        Status.Text = "Signed out.";
    }
    private async void SelectChannel(object sender, SelectionChangedEventArgs e)
    {
        ++revision;
        Messages.ItemsSource = null;
        Draft.Clear();
        ChannelTitle.Text = Channels.SelectedItem is Channel channel ? $"# {channel.name}" : "Choose a channel";
        SendButton.IsEnabled = Channels.SelectedItem is Channel;
        await Refresh();
    }
    private async Task Refresh()
    {
        var client = api;
        if (refreshing || client == null || Channels.SelectedItem is not Channel channel) return;
        var version = revision;
        refreshing = true;
        try
        {
            // Re-read visibility on each poll, so revoked private channels disappear.
            var channels = (await client.Channels()).Where(c => c.kind != "voice").OrderBy(c => c.position).ToList();
            if (version != revision || client != api) return;
            if (!channels.Any(c => c.id == channel.id))
            {
                Channels.ItemsSource = channels;
                Messages.ItemsSource = null;
                Status.Text = "This channel is no longer available.";
                return;
            }
            if (Channels.ItemsSource is not List<Channel> previous || !previous.SequenceEqual(channels))
            {
                Channels.ItemsSource = channels;
                Channels.SelectedItem = channels.Find(c => c.id == channel.id);
                return;
            }
            var messages = await client.Messages(channel.id);
            if (version != revision || client != api) return;
            if (Messages.ItemsSource is List<Message> old && System.Text.Json.JsonSerializer.Serialize(old) == System.Text.Json.JsonSerializer.Serialize(messages)) return;
            var firstLoad = Messages.ItemsSource == null;
            Messages.ItemsSource = messages;
            if (firstLoad && messages.Count > 0) Messages.ScrollIntoView(messages[^1]);
            Status.Text = "Connected · refreshed " + DateTime.Now.ToShortTimeString();
        }
        catch (Exception error)
        {
            if (version != revision || client != api) return;
            if (error is SessionExpiredException) SignOut();
            Status.Text = error.Message;
        }
        finally { refreshing = false; }
    }
    private async void Send(object sender, RoutedEventArgs e)
    {
        var client = api;
        if (client == null || Channels.SelectedItem is not Channel channel || string.IsNullOrWhiteSpace(Draft.Text)) return;
        var content = Draft.Text;
        var version = revision;
        SendButton.IsEnabled = false;
        try
        {
            await client.Send(channel.id, content.Trim());
            if (version != revision || client != api) return;
            if (Draft.Text == content) Draft.Clear();
            await Refresh();
        }
        catch (Exception error)
        {
            if (version != revision || client != api) return;
            if (error is SessionExpiredException) SignOut();
            Status.Text = error.Message;
        }
        finally { if (version == revision && client == api) SendButton.IsEnabled = true; }
    }
}

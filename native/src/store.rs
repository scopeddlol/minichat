//! Application state.
//!
//! One owner for everything the UI draws, plus the rules for folding gateway
//! events into it. The UI holds no state of its own — it is rebuilt from here
//! whenever something changes — which keeps "what is true" in one place and
//! makes the event handling testable without a window.

use std::collections::HashMap;

use crate::api::types::*;
use crate::gateway::Frame;

/// The most messages kept per channel.
///
/// The web client keeps every message it has ever loaded, in a map that is
/// never trimmed, and renders all of them into the DOM. Visit eight channels,
/// scroll back in a few, and that alone is hundreds of megabytes. A native
/// client has no excuse to repeat it: older messages are a fetch away, and
/// the window here is far more than anyone scrolls without paging.
pub const MAX_MESSAGES_PER_CHANNEL: usize = 300;
/// The most channels whose history is kept at once. Beyond this the
/// least-recently-viewed channel's messages are dropped.
pub const MAX_CACHED_CHANNELS: usize = 12;

#[derive(Default)]
pub struct Store {
    pub instance: Instance,
    pub me: Me,
    pub permissions: u64,

    pub roles: Vec<Role>,
    pub categories: Vec<Category>,
    pub channels: Vec<Channel>,
    pub members: Vec<Member>,
    pub emojis: Vec<Emoji>,
    pub voice_states: Vec<VoiceState>,
    /// Read by the direct-message inbox, which is not built yet.
    #[allow(dead_code)]
    pub relationships: Vec<Relationship>,

    /// Per-channel permission bits, which may narrow the instance-wide ones.
    pub channel_permissions: HashMap<String, u64>,

    pub messages: HashMap<String, Vec<Message>>,
    /// Channels whose history is loaded, most recently viewed last.
    recency: Vec<String>,
    pub unread: HashMap<String, i64>,
    pub mentions: HashMap<String, i64>,
    pub last_read: HashMap<String, String>,
    /// Members typing in a channel, with the moment they were last seen to.
    pub typing: HashMap<String, Vec<(String, i64)>>,
    pub collapsed_categories: Vec<String>,

    // --- direct messages -------------------------------------------------
    pub conversations: Vec<Conversation>,
    /// Per-conversation history, capped the same way channels are.
    pub direct: HashMap<String, Vec<DirectMessage>>,
    pub selected_conversation: String,
    pub inbox_open: bool,
    /// Calls that are ringing or connected, from the gateway.
    pub calls: Vec<DirectCall>,

    pub selected_channel: String,
    pub voice_enabled: bool,
    pub livekit_url: String,
    pub ready: bool,
    /// From `/api/admin/instance`, which only an administrator can read.
    pub public_url: String,
    pub notifications: NotificationPreferences,
}

impl Store {
    /// Replace everything from the gateway's opening frame.
    pub fn apply_ready(&mut self, payload: ReadyPayload) {
        self.permissions = crate::perms::parse(&payload.permissions);
        self.channel_permissions = payload
            .channel_permissions
            .iter()
            .map(|(id, bits)| (id.clone(), crate::perms::parse(bits)))
            .collect();
        self.last_read = payload
            .read_state
            .iter()
            .map(|entry| (entry.channel_id.clone(), entry.last_read_id.clone()))
            .collect();

        self.instance = payload.instance;
        self.me = payload.me;
        self.roles = payload.roles;
        self.categories = payload.categories;
        self.channels = payload.channels;
        self.members = payload.members;
        self.emojis = payload.emojis;
        self.voice_states = payload.voice_states;
        self.notifications = payload.notifications;
        self.unread = payload.unread;
        self.mentions = payload.mentions;
        self.voice_enabled = payload.voice_enabled;
        self.livekit_url = payload.livekit_url;
        self.ready = true;

        self.sort();

        // A reconnect re-sends READY. Anything selected that no longer exists
        // (deleted, or access revoked) falls back to the first visible
        // channel rather than leaving the pane pointed at nothing.
        if !self.channels.iter().any(|c| c.id == self.selected_channel) {
            self.selected_channel = self.first_text_channel().unwrap_or_default();
        }
    }

    fn sort(&mut self) {
        self.categories.sort_by_key(|c| c.position);
        self.channels.sort_by_key(|c| c.position);
        // Highest role first, which is the order the hierarchy reads in.
        self.roles.sort_by(|a, b| b.position.cmp(&a.position));
        self.members.sort_by_key(|m| m.name().to_lowercase());
    }

    pub fn first_text_channel(&self) -> Option<String> {
        self.channels
            .iter()
            .find(|c| c.kind != ChannelKind::Voice)
            .map(|c| c.id.clone())
    }

    pub fn channel(&self, id: &str) -> Option<&Channel> {
        self.channels.iter().find(|c| c.id == id)
    }

    pub fn member(&self, id: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.id == id)
    }

    /// The permissions in force in a channel: the per-channel bits when the
    /// server sent them, otherwise the instance-wide ones.
    pub fn permissions_in(&self, channel: &str) -> u64 {
        self.channel_permissions
            .get(channel)
            .copied()
            .unwrap_or(self.permissions)
    }

    pub fn can_send_in(&self, channel: &str) -> bool {
        crate::perms::can(self.permissions_in(channel), crate::perms::SEND_MESSAGES)
    }

    pub fn messages_in(&self, channel: &str) -> &[Message] {
        self.messages.get(channel).map(Vec::as_slice).unwrap_or(&[])
    }

    /// A channel's messages with blocked members' removed.
    ///
    /// Blocking is enforced on the server for direct messages, but a channel
    /// is shared — the server cannot drop a message for one reader. So the
    /// client hides them, which is what blocking means from the reader's
    /// side. Kept separate from `messages_in` so the raw list is still
    /// available to anything that needs it, such as resolving a reply.
    pub fn visible_messages_in(&self, channel: &str) -> Vec<Message> {
        self.messages_in(channel)
            .iter()
            .filter(|message| {
                message
                    .author_id()
                    .map(|id| !self.is_blocked(id))
                    .unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    /// Record that a channel's history was used, and drop the least recently
    /// used one once too many are held.
    pub fn touch(&mut self, channel: &str) {
        self.recency.retain(|id| id != channel);
        self.recency.push(channel.to_string());

        while self.recency.len() > MAX_CACHED_CHANNELS {
            let evicted = self.recency.remove(0);
            if evicted != self.selected_channel {
                self.messages.remove(&evicted);
            }
        }
    }

    /// Replace a channel's history with a freshly fetched page.
    pub fn set_messages(&mut self, channel: &str, mut messages: Vec<Message>) {
        messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        trim(&mut messages);
        self.messages.insert(channel.to_string(), messages);
        self.touch(channel);
    }

    /// Prepend an older page, keeping the newest end of the window.
    ///
    /// Used by scroll-back, which the message list does not offer yet.
    #[allow(dead_code)]
    pub fn prepend_messages(&mut self, channel: &str, mut older: Vec<Message>) {
        older.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        let existing = self.messages.entry(channel.to_string()).or_default();
        older.retain(|old| !existing.iter().any(|m| m.id == old.id));
        older.append(existing);
        *existing = older;
        // Trimming from the front here would undo the fetch, so the cap is
        // applied to the *end* instead: paging back keeps what was paged to.
        if existing.len() > MAX_MESSAGES_PER_CHANNEL {
            existing.truncate(MAX_MESSAGES_PER_CHANNEL);
        }
    }

    pub fn upsert_message(&mut self, message: Message) {
        let channel = message.channel_id.clone();
        let list = self.messages.entry(channel).or_default();
        match list.iter_mut().find(|m| m.id == message.id) {
            Some(existing) => *existing = message,
            None => {
                list.push(message);
                trim(list);
            }
        }
    }

    /// Replace an optimistic message with the one the server stored.
    ///
    /// The gateway routinely delivers the real message before the POST that
    /// created it returns, so the stored one may already be in the list. The
    /// placeholder is then dropped rather than overwritten, which is what
    /// stops the message appearing twice.
    pub fn confirm_message(&mut self, channel: &str, temp_id: &str, saved: Message) {
        let list = self.messages.entry(channel.to_string()).or_default();
        let already_present = list.iter().any(|m| m.id == saved.id);

        list.retain(|m| m.id != temp_id);
        if !already_present {
            list.push(saved);
            trim(list);
        }
    }

    pub fn fail_message(&mut self, channel: &str, temp_id: &str) {
        if let Some(list) = self.messages.get_mut(channel) {
            if let Some(message) = list.iter_mut().find(|m| m.id == temp_id) {
                message.pending = false;
                message.failed = true;
            }
        }
    }

    pub fn remove_message(&mut self, channel: &str, id: &str) {
        if let Some(list) = self.messages.get_mut(channel) {
            list.retain(|m| m.id != id);
        }
    }

    /// The first message that arrived after the channel was last read.
    pub fn first_unread(&self, channel: &str) -> Option<&str> {
        let last_read = self.last_read.get(channel)?;
        let list = self.messages.get(channel)?;
        let position = list.iter().position(|m| &m.id == last_read)?;
        list.get(position + 1).map(|m| m.id.as_str())
    }

    pub fn mark_read(&mut self, channel: &str) {
        self.unread.remove(channel);
        self.mentions.remove(channel);
        if let Some(last) = self.messages_in(channel).last() {
            self.last_read.insert(channel.to_string(), last.id.clone());
        }
    }

    /// The call ringing for me that I have not answered, if any.
    ///
    /// Mine-as-caller is excluded: the one I placed is shown as "calling",
    /// not as an incoming call to accept.
    pub fn incoming_call(&self) -> Option<&DirectCall> {
        self.calls
            .iter()
            .find(|c| c.status == CallStatus::Ringing && c.caller_id != self.me.member.id)
    }

    /// The call I am in or placing, if any.
    pub fn my_call(&self) -> Option<&DirectCall> {
        self.calls.iter().find(|c| {
            c.status == CallStatus::Accepted
                || (c.status == CallStatus::Ringing && c.caller_id == self.me.member.id)
        })
    }

    /// The member on the other end of a call.
    pub fn call_peer(&self, call: &DirectCall) -> Option<&Member> {
        if call.caller_id != self.me.member.id {
            return self.member(&call.caller_id);
        }
        self.peer(&call.conversation_id)
    }

    /// How I relate to another member, from my point of view.
    pub fn relationship(&self, user: &str) -> RelationshipKind {
        self.relationships
            .iter()
            .find(|r| r.user_id == user)
            .map(|r| r.kind)
            .unwrap_or(RelationshipKind::None)
    }

    pub fn is_favourite(&self, user: &str) -> bool {
        self.relationships
            .iter()
            .any(|r| r.user_id == user && r.favourite)
    }

    /// Whether a member is blocked, which hides their messages.
    pub fn is_blocked(&self, user: &str) -> bool {
        self.relationship(user) == RelationshipKind::Blocked
    }

    /// Members in a given relationship, in the order the member list is in.
    pub fn related(&self, kind: RelationshipKind) -> Vec<&Member> {
        self.relationships
            .iter()
            .filter(|r| r.kind == kind)
            .filter_map(|r| self.member(&r.user_id))
            .collect()
    }

    /// Friend requests waiting on an answer, which is what the inbox badges.
    pub fn incoming_requests(&self) -> usize {
        self.relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Incoming)
            .count()
    }

    /// Apply a relationship change locally so the UI answers immediately
    /// rather than waiting for the refetch the server asks for.
    pub fn set_relationship(&mut self, user: &str, kind: RelationshipKind) {
        match self.relationships.iter_mut().find(|r| r.user_id == user) {
            Some(existing) => existing.kind = kind,
            None => self.relationships.push(Relationship {
                user_id: user.to_string(),
                kind,
                favourite: false,
            }),
        }
        // `None` means no relationship at all, so it is removed rather than
        // left as a row that says "nothing".
        if kind == RelationshipKind::None {
            self.relationships
                .retain(|r| r.user_id != user || r.favourite);
            if let Some(row) = self.relationships.iter_mut().find(|r| r.user_id == user) {
                row.kind = RelationshipKind::None;
            }
        }
    }

    pub fn set_favourite(&mut self, user: &str, favourite: bool) {
        match self.relationships.iter_mut().find(|r| r.user_id == user) {
            Some(existing) => existing.favourite = favourite,
            None => self.relationships.push(Relationship {
                user_id: user.to_string(),
                kind: RelationshipKind::None,
                favourite,
            }),
        }
    }

    /// The instance's public URL, for links that leave the client.
    ///
    /// Only `/api/admin/instance` carries it, so before an admin has looked
    /// at that screen this is empty — callers omit the link rather than
    /// producing a broken one.
    pub fn instance_public_url(&self) -> String {
        self.public_url.clone()
    }

    pub fn total_unread(&self) -> i64 {
        self.unread.values().sum()
    }

    // --- direct messages ----------------------------------------------

    pub fn conversation(&self, id: &str) -> Option<&Conversation> {
        self.conversations.iter().find(|c| c.id == id)
    }

    /// The member on the other end of a conversation.
    pub fn peer(&self, conversation: &str) -> Option<&Member> {
        let peer_id = &self.conversation(conversation)?.peer_id;
        self.member(peer_id)
    }

    pub fn direct_in(&self, conversation: &str) -> &[DirectMessage] {
        self.direct
            .get(conversation)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn set_direct(&mut self, conversation: &str, mut messages: Vec<DirectMessage>) {
        messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        if messages.len() > MAX_MESSAGES_PER_CHANNEL {
            let excess = messages.len() - MAX_MESSAGES_PER_CHANNEL;
            messages.drain(..excess);
        }
        self.direct.insert(conversation.to_string(), messages);
    }

    pub fn upsert_direct(&mut self, message: DirectMessage) {
        let conversation = message.conversation_id.clone();
        let list = self.direct.entry(conversation).or_default();
        match list.iter_mut().find(|m| m.id == message.id) {
            Some(existing) => *existing = message,
            None => {
                list.push(message);
                if list.len() > MAX_MESSAGES_PER_CHANNEL {
                    let excess = list.len() - MAX_MESSAGES_PER_CHANNEL;
                    list.drain(..excess);
                }
            }
        }
    }

    /// Replace an optimistic direct message with the stored one, with the
    /// same duplicate guard the channel path has.
    pub fn confirm_direct(&mut self, conversation: &str, temp_id: &str, saved: DirectMessage) {
        let list = self.direct.entry(conversation.to_string()).or_default();
        let already_present = list.iter().any(|m| m.id == saved.id);
        list.retain(|m| m.id != temp_id);
        if !already_present {
            list.push(saved);
        }
    }

    pub fn fail_direct(&mut self, conversation: &str, temp_id: &str) {
        if let Some(list) = self.direct.get_mut(conversation) {
            if let Some(message) = list.iter_mut().find(|m| m.id == temp_id) {
                message.pending = false;
                message.failed = true;
            }
        }
    }

    /// Total unread across every conversation, for the sidebar's badge.
    pub fn direct_unread(&self) -> i64 {
        self.conversations.iter().map(|c| c.unread).sum()
    }

    pub fn mark_conversation_read(&mut self, conversation: &str) {
        if let Some(entry) = self.conversations.iter_mut().find(|c| c.id == conversation) {
            entry.unread = 0;
        }
    }

    pub fn toggle_category(&mut self, id: &str) {
        match self.collapsed_categories.iter().position(|c| c == id) {
            Some(index) => {
                self.collapsed_categories.remove(index);
            }
            None => self.collapsed_categories.push(id.to_string()),
        }
    }

    /// Members typing in a channel, excluding me and anyone stale.
    pub fn typing_names(&self, channel: &str, now: i64) -> Vec<String> {
        const STALE_AFTER: i64 = 8;
        self.typing
            .get(channel)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|(id, at)| id != &self.me.member.id && now - at < STALE_AFTER)
                    .filter_map(|(id, _)| self.member(id).map(|m| m.name().to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Fold one gateway event in. Returns true when the UI needs rebuilding.
    pub fn apply_event(&mut self, frame: &Frame, now: i64) -> bool {
        let data = &frame.data;

        match frame.event.as_str() {
            "MESSAGE_CREATE" => {
                let Some(message) = decode::<Message>(data) else {
                    return false;
                };
                let channel = message.channel_id.clone();
                let mine = message.author_id() == Some(self.me.member.id.as_str());

                // Only count it as unread when it is not mine and not in the
                // channel being looked at.
                if !mine && channel != self.selected_channel {
                    *self.unread.entry(channel.clone()).or_insert(0) += 1;
                }
                // The author has evidently stopped typing.
                if let (Some(entries), Some(author)) =
                    (self.typing.get_mut(&channel), message.author_id())
                {
                    entries.retain(|(id, _)| id != author);
                }
                self.upsert_message(message);
                true
            }

            "MESSAGE_UPDATE" => match decode::<Message>(data) {
                Some(message) => {
                    self.upsert_message(message);
                    true
                }
                None => false,
            },

            "MESSAGE_DELETE" => {
                let channel = data["channel_id"].as_str().unwrap_or_default().to_string();
                let id = data["id"].as_str().unwrap_or_default();
                self.remove_message(&channel, id);
                true
            }

            "REACTION_UPDATE" => {
                let channel = data["channel_id"].as_str().unwrap_or_default();
                let id = data["message_id"].as_str().unwrap_or_default();
                let groups: Vec<ReactionGroup> =
                    serde_json::from_value(data["reactions"].clone()).unwrap_or_default();
                if let Some(list) = self.messages.get_mut(channel) {
                    if let Some(message) = list.iter_mut().find(|m| m.id == id) {
                        message.reactions = groups;
                        return true;
                    }
                }
                false
            }

            "TYPING_START" => {
                let channel = data["channel_id"].as_str().unwrap_or_default().to_string();
                let user = data["user_id"].as_str().unwrap_or_default().to_string();
                if user == self.me.member.id {
                    return false;
                }
                let entries = self.typing.entry(channel).or_default();
                entries.retain(|(id, _)| id != &user);
                entries.push((user, now));
                true
            }

            "MENTION_ADD" => {
                let channel = data["channel_id"].as_str().unwrap_or_default().to_string();
                if channel != self.selected_channel {
                    *self.mentions.entry(channel).or_insert(0) += 1;
                }
                true
            }

            "DIRECT_MESSAGE" => {
                let Some(message) = decode::<DirectMessage>(data) else {
                    return false;
                };
                let conversation = message.conversation_id.clone();
                let mine = message.author_id == self.me.member.id;

                // A conversation the client has not seen before arrives with
                // its first message; record it so the inbox lists it without
                // waiting for a refetch.
                if self.conversation(&conversation).is_none() {
                    self.conversations.push(Conversation {
                        id: conversation.clone(),
                        peer_id: if mine {
                            String::new()
                        } else {
                            message.author_id.clone()
                        },
                        last_content: Some(message.content.clone()),
                        unread: 0,
                    });
                }

                let reading_it = self.inbox_open && self.selected_conversation == conversation;
                if let Some(entry) = self.conversations.iter_mut().find(|c| c.id == conversation) {
                    entry.last_content = Some(message.content.clone());
                    if !mine && !reading_it {
                        entry.unread += 1;
                    }
                }

                self.upsert_direct(message);
                true
            }

            "DIRECT_CALL" => {
                let Some(call) = decode::<DirectCall>(data) else {
                    return false;
                };
                // An ended call is removed rather than kept as a row saying
                // "ended": nothing in the UI wants to know about one.
                self.calls.retain(|c| c.id != call.id);
                if call.is_live() {
                    self.calls.push(call);
                }
                true
            }

            "DIRECT_READ" => {
                let conversation = data["conversation_id"].as_str().unwrap_or_default();
                self.mark_conversation_read(conversation);
                true
            }

            "RELATIONSHIPS_STALE" => {
                // The server is telling the client its friends list is out of
                // date. Refetching is the caller's job; this only reports
                // that something changed.
                true
            }

            "CHANNEL_CREATE" | "CHANNEL_UPDATE" => match decode::<Channel>(data) {
                Some(channel) => {
                    match self.channels.iter_mut().find(|c| c.id == channel.id) {
                        Some(existing) => *existing = channel,
                        None => self.channels.push(channel),
                    }
                    self.sort();
                    true
                }
                None => false,
            },

            "CHANNEL_DELETE" => {
                let id = data["id"].as_str().unwrap_or_default();
                self.channels.retain(|c| c.id != id);
                self.messages.remove(id);
                if self.selected_channel == id {
                    self.selected_channel = self.first_text_channel().unwrap_or_default();
                }
                true
            }

            "CATEGORY_CREATE" | "CATEGORY_UPDATE" => match decode::<Category>(data) {
                Some(category) => {
                    match self.categories.iter_mut().find(|c| c.id == category.id) {
                        Some(existing) => *existing = category,
                        None => self.categories.push(category),
                    }
                    self.sort();
                    true
                }
                None => false,
            },

            "CATEGORY_DELETE" => {
                let id = data["id"].as_str().unwrap_or_default();
                self.categories.retain(|c| c.id != id);
                true
            }

            "MEMBER_ADD" | "MEMBER_UPDATE" => match decode::<Member>(data) {
                Some(member) => {
                    if member.id == self.me.member.id {
                        self.me.member = member.clone();
                    }
                    match self.members.iter_mut().find(|m| m.id == member.id) {
                        Some(existing) => *existing = member,
                        None => self.members.push(member),
                    }
                    self.sort();
                    true
                }
                None => false,
            },

            "MEMBER_REMOVE" => {
                let id = data["id"]
                    .as_str()
                    .or_else(|| data["user_id"].as_str())
                    .unwrap_or_default();
                self.members.retain(|m| m.id != id);
                true
            }

            "PRESENCE_UPDATE" => {
                let id = data["user_id"].as_str().unwrap_or_default();
                let presence: Presence =
                    serde_json::from_value(data["presence"].clone()).unwrap_or_default();
                if let Some(member) = self.members.iter_mut().find(|m| m.id == id) {
                    member.presence = presence;
                    if let Some(status) = data["custom_status"].as_str() {
                        member.custom_status = status.to_string();
                    }
                    return true;
                }
                false
            }

            "ROLE_CREATE" | "ROLE_UPDATE" => match decode::<Role>(data) {
                Some(role) => {
                    match self.roles.iter_mut().find(|r| r.id == role.id) {
                        Some(existing) => *existing = role,
                        None => self.roles.push(role),
                    }
                    self.sort();
                    true
                }
                None => false,
            },

            "ROLE_DELETE" => {
                let id = data["id"].as_str().unwrap_or_default();
                self.roles.retain(|r| r.id != id);
                true
            }

            "EMOJI_CREATE" | "EMOJI_UPDATE" => match decode::<Emoji>(data) {
                Some(emoji) => {
                    match self.emojis.iter_mut().find(|e| e.id == emoji.id) {
                        Some(existing) => *existing = emoji,
                        None => self.emojis.push(emoji),
                    }
                    true
                }
                None => false,
            },

            "EMOJI_DELETE" => {
                let id = data["id"].as_str().unwrap_or_default();
                self.emojis.retain(|e| e.id != id);
                true
            }

            "VOICE_STATE_UPDATE" => match decode::<VoiceState>(data) {
                Some(state) => {
                    match self
                        .voice_states
                        .iter_mut()
                        .find(|s| s.user_id == state.user_id)
                    {
                        Some(existing) => *existing = state,
                        None => self.voice_states.push(state),
                    }
                    true
                }
                None => false,
            },

            "VOICE_STATE_LEAVE" | "VOICE_FORCE_DISCONNECT" => {
                let id = data["user_id"].as_str().unwrap_or_default();
                self.voice_states.retain(|s| s.user_id != id);
                true
            }

            "INSTANCE_UPDATE" => match decode::<Instance>(data) {
                Some(instance) => {
                    self.instance = instance;
                    true
                }
                None => false,
            },

            // The member's access changed: which channels they can see, and
            // what they may do in them. The server sends the new answer
            // rather than expecting the client to work it out.
            "ACCESS_UPDATE" => {
                if let Some(bits) = data["permissions"].as_str() {
                    self.permissions = crate::perms::parse(bits);
                }
                if let Some(map) = data["channel_permissions"].as_object() {
                    self.channel_permissions = map
                        .iter()
                        .map(|(id, bits)| {
                            (
                                id.clone(),
                                crate::perms::parse(bits.as_str().unwrap_or("0")),
                            )
                        })
                        .collect();
                }
                if let Some(channels) = data["channels"].as_array() {
                    self.channels = channels
                        .iter()
                        .filter_map(|c| serde_json::from_value(c.clone()).ok())
                        .collect();
                    self.sort();
                    if !self.channels.iter().any(|c| c.id == self.selected_channel) {
                        self.selected_channel = self.first_text_channel().unwrap_or_default();
                    }
                }
                true
            }

            _ => false,
        }
    }
}

/// Deserialise an event payload, treating anything unexpected as absent.
///
/// A server newer than this client can send a shape it does not understand;
/// that must cost the event, never the connection.
fn decode<T: serde::de::DeserializeOwned>(value: &serde_json::Value) -> Option<T> {
    serde_json::from_value(value.clone()).ok()
}

/// Keep the newest `MAX_MESSAGES_PER_CHANNEL`.
fn trim(messages: &mut Vec<Message>) {
    if messages.len() > MAX_MESSAGES_PER_CHANNEL {
        let excess = messages.len() - MAX_MESSAGES_PER_CHANNEL;
        messages.drain(..excess);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn frame(event: &str, data: serde_json::Value) -> Frame {
        Frame {
            event: event.into(),
            data,
        }
    }

    fn store_with_me(id: &str) -> Store {
        let mut store = Store::default();
        store.me.member.id = id.into();
        store
    }

    fn message(id: &str, channel: &str, author: &str) -> serde_json::Value {
        json!({
            "id": id,
            "channel_id": channel,
            "author": { "id": author, "username": author, "display_name": author },
            "content": "hello",
            "created_at": "2026-03-04 10:00:00"
        })
    }

    #[test]
    fn an_incoming_message_lands_in_its_channel() {
        let mut store = store_with_me("me");
        assert!(store.apply_event(&frame("MESSAGE_CREATE", message("1", "general", "ada")), 0));
        assert_eq!(store.messages_in("general").len(), 1);
    }

    #[test]
    fn unread_counts_only_what_is_not_mine_and_not_on_screen() {
        let mut store = store_with_me("me");
        store.selected_channel = "general".into();

        // In the open channel: no badge.
        store.apply_event(&frame("MESSAGE_CREATE", message("1", "general", "ada")), 0);
        assert_eq!(store.unread.get("general"), None);

        // Elsewhere: a badge.
        store.apply_event(&frame("MESSAGE_CREATE", message("2", "design", "ada")), 0);
        assert_eq!(store.unread.get("design"), Some(&1));

        // My own message never counts, wherever it lands.
        store.apply_event(&frame("MESSAGE_CREATE", message("3", "design", "me")), 0);
        assert_eq!(store.unread.get("design"), Some(&1));
    }

    #[test]
    fn history_is_capped_rather_than_grown_without_limit() {
        let mut store = store_with_me("me");
        for index in 0..MAX_MESSAGES_PER_CHANNEL + 50 {
            store.apply_event(
                &frame(
                    "MESSAGE_CREATE",
                    message(&index.to_string(), "general", "ada"),
                ),
                0,
            );
        }
        assert_eq!(store.messages_in("general").len(), MAX_MESSAGES_PER_CHANNEL);
        // The newest are the ones kept.
        let last = store.messages_in("general").last().unwrap();
        assert_eq!(last.id, (MAX_MESSAGES_PER_CHANNEL + 49).to_string());
    }

    #[test]
    fn the_least_recently_viewed_channel_is_evicted() {
        let mut store = store_with_me("me");
        store.selected_channel = "keep-me".into();
        store.set_messages("keep-me", vec![Message::default()]);

        for index in 0..MAX_CACHED_CHANNELS + 3 {
            store.set_messages(&format!("c{index}"), vec![Message::default()]);
        }
        assert!(store.messages.len() <= MAX_CACHED_CHANNELS + 1);
        // The open channel survives eviction whatever its position.
        assert!(store.messages.contains_key("keep-me"));
        assert!(!store.messages.contains_key("c0"));
    }

    #[test]
    fn an_optimistic_message_is_replaced_by_the_stored_one() {
        let mut store = store_with_me("me");
        store.upsert_message(Message {
            id: "pending-1".into(),
            channel_id: "general".into(),
            pending: true,
            ..Default::default()
        });
        store.confirm_message(
            "general",
            "pending-1",
            Message {
                id: "real-1".into(),
                channel_id: "general".into(),
                ..Default::default()
            },
        );
        let list = store.messages_in("general");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "real-1");
        assert!(!list[0].pending);
    }

    #[test]
    fn a_message_that_arrived_by_gateway_first_does_not_duplicate() {
        let mut store = store_with_me("me");
        store.upsert_message(Message {
            id: "pending-1".into(),
            channel_id: "general".into(),
            pending: true,
            ..Default::default()
        });
        // The gateway delivered it before the POST returned.
        store.upsert_message(Message {
            id: "real-1".into(),
            channel_id: "general".into(),
            ..Default::default()
        });
        store.confirm_message(
            "general",
            "pending-1",
            Message {
                id: "real-1".into(),
                channel_id: "general".into(),
                ..Default::default()
            },
        );
        assert_eq!(store.messages_in("general").len(), 1);
    }

    #[test]
    fn a_failed_send_is_marked_rather_than_dropped() {
        let mut store = store_with_me("me");
        store.upsert_message(Message {
            id: "pending-1".into(),
            channel_id: "general".into(),
            pending: true,
            ..Default::default()
        });
        store.fail_message("general", "pending-1");
        let message = &store.messages_in("general")[0];
        assert!(message.failed && !message.pending);
    }

    #[test]
    fn a_reaction_update_replaces_the_groups() {
        let mut store = store_with_me("me");
        store.apply_event(&frame("MESSAGE_CREATE", message("1", "general", "ada")), 0);
        assert!(store.apply_event(
            &frame(
                "REACTION_UPDATE",
                json!({
                    "channel_id": "general",
                    "message_id": "1",
                    "reactions": [{ "emoji": "\u{1f389}", "count": 2, "me": true }]
                })
            ),
            0
        ));
        assert_eq!(store.messages_in("general")[0].reactions[0].count, 2);
    }

    #[test]
    fn typing_excludes_me_and_forgets_stale_entries() {
        let mut store = store_with_me("me");
        store.members = vec![Member {
            id: "ada".into(),
            username: "ada".into(),
            display_name: "Ada".into(),
            ..Default::default()
        }];

        store.apply_event(
            &frame(
                "TYPING_START",
                json!({ "channel_id": "general", "user_id": "ada" }),
            ),
            1000,
        );
        assert_eq!(store.typing_names("general", 1002), vec!["Ada"]);
        // Nine seconds later it has expired.
        assert!(store.typing_names("general", 1009).is_empty());

        // My own typing is never echoed back at me.
        assert!(!store.apply_event(
            &frame(
                "TYPING_START",
                json!({ "channel_id": "general", "user_id": "me" })
            ),
            1000
        ));
    }

    #[test]
    fn deleting_the_open_channel_moves_the_selection() {
        let mut store = store_with_me("me");
        store.channels = vec![
            Channel {
                id: "a".into(),
                position: 0,
                ..Default::default()
            },
            Channel {
                id: "b".into(),
                position: 1,
                ..Default::default()
            },
        ];
        store.selected_channel = "a".into();
        store.apply_event(&frame("CHANNEL_DELETE", json!({ "id": "a" })), 0);
        assert_eq!(store.selected_channel, "b");
    }

    #[test]
    fn access_update_narrows_permissions_and_the_channel_list() {
        let mut store = store_with_me("me");
        store.channels = vec![Channel {
            id: "secret".into(),
            ..Default::default()
        }];
        store.selected_channel = "secret".into();

        store.apply_event(
            &frame(
                "ACCESS_UPDATE",
                json!({
                    "permissions": "3",
                    "channel_permissions": { "open": "1" },
                    "channels": [{ "id": "open", "name": "open" }]
                }),
            ),
            0,
        );
        assert_eq!(
            store.permissions,
            crate::perms::VIEW_CHANNELS | crate::perms::SEND_MESSAGES
        );
        assert_eq!(store.selected_channel, "open");
        assert_eq!(store.permissions_in("open"), crate::perms::VIEW_CHANNELS);
    }

    #[test]
    fn a_channel_with_no_override_inherits_the_instance_permissions() {
        let mut store = store_with_me("me");
        store.permissions = crate::perms::VIEW_CHANNELS | crate::perms::SEND_MESSAGES;
        assert!(store.can_send_in("anything"));

        store
            .channel_permissions
            .insert("readonly".into(), crate::perms::VIEW_CHANNELS);
        assert!(!store.can_send_in("readonly"));
    }

    #[test]
    fn the_unread_marker_points_at_the_first_message_after_the_read_mark() {
        let mut store = store_with_me("me");
        store.set_messages(
            "general",
            vec![
                Message {
                    id: "1".into(),
                    created_at: "2026-03-04 10:00:00".into(),
                    ..Default::default()
                },
                Message {
                    id: "2".into(),
                    created_at: "2026-03-04 10:01:00".into(),
                    ..Default::default()
                },
                Message {
                    id: "3".into(),
                    created_at: "2026-03-04 10:02:00".into(),
                    ..Default::default()
                },
            ],
        );
        store.last_read.insert("general".into(), "1".into());
        assert_eq!(store.first_unread("general"), Some("2"));

        // Caught up: nothing is marked.
        store.mark_read("general");
        assert_eq!(store.first_unread("general"), None);
    }

    #[test]
    fn a_direct_message_lands_and_counts_as_unread_unless_it_is_open() {
        let mut store = store_with_me("me");
        let arrive = |id: &str, author: &str| {
            frame(
                "DIRECT_MESSAGE",
                json!({
                    "id": id,
                    "conversation_id": "c1",
                    "author_id": author,
                    "content": "hello",
                    "created_at": "2026-03-04 10:00:00"
                }),
            )
        };

        // The inbox is closed, so it counts.
        assert!(store.apply_event(&arrive("1", "ada"), 0));
        assert_eq!(store.direct_in("c1").len(), 1);
        assert_eq!(store.direct_unread(), 1);
        // The conversation was learned from the message itself.
        assert_eq!(
            store.conversation("c1").map(|c| c.peer_id.as_str()),
            Some("ada")
        );

        // Reading that conversation, it does not.
        store.inbox_open = true;
        store.selected_conversation = "c1".into();
        store.apply_event(&arrive("2", "ada"), 0);
        assert_eq!(store.direct_unread(), 1);

        // My own never counts.
        store.inbox_open = false;
        store.apply_event(&arrive("3", "me"), 0);
        assert_eq!(store.direct_unread(), 1);
    }

    #[test]
    fn a_read_receipt_clears_the_conversations_badge() {
        let mut store = store_with_me("me");
        store.conversations = vec![Conversation {
            id: "c1".into(),
            unread: 4,
            ..Default::default()
        }];
        assert_eq!(store.direct_unread(), 4);
        store.apply_event(&frame("DIRECT_READ", json!({ "conversation_id": "c1" })), 0);
        assert_eq!(store.direct_unread(), 0);
    }

    #[test]
    fn an_optimistic_direct_message_does_not_duplicate_either() {
        let mut store = store_with_me("me");
        store.upsert_direct(DirectMessage {
            id: "pending-1".into(),
            conversation_id: "c1".into(),
            pending: true,
            ..Default::default()
        });
        store.upsert_direct(DirectMessage {
            id: "real-1".into(),
            conversation_id: "c1".into(),
            ..Default::default()
        });
        store.confirm_direct(
            "c1",
            "pending-1",
            DirectMessage {
                id: "real-1".into(),
                conversation_id: "c1".into(),
                ..Default::default()
            },
        );
        assert_eq!(store.direct_in("c1").len(), 1);
    }

    #[test]
    fn a_ringing_call_is_incoming_only_when_someone_else_placed_it() {
        let mut store = store_with_me("me");
        let ring = |id: &str, caller: &str, status: &str| {
            frame(
                "DIRECT_CALL",
                json!({
                    "id": id,
                    "conversation_id": "c1",
                    "caller_id": caller,
                    "status": status
                }),
            )
        };

        // Someone calling me is an incoming call to answer.
        store.apply_event(&ring("call1", "ada", "ringing"), 0);
        assert_eq!(store.incoming_call().map(|c| c.id.as_str()), Some("call1"));

        // My own outgoing call is not something to accept.
        store.apply_event(&ring("call1", "ada", "ended"), 0);
        store.apply_event(&ring("call2", "me", "ringing"), 0);
        assert!(store.incoming_call().is_none());
        assert_eq!(store.my_call().map(|c| c.id.as_str()), Some("call2"));

        // Ending it clears the row rather than keeping a dead one.
        store.apply_event(&ring("call2", "me", "ended"), 0);
        assert!(store.calls.is_empty());
        assert!(store.my_call().is_none());
    }

    #[test]
    fn an_accepted_call_is_mine_whoever_placed_it() {
        let mut store = store_with_me("me");
        store.apply_event(
            &frame(
                "DIRECT_CALL",
                json!({"id":"c","conversation_id":"c1","caller_id":"ada","status":"accepted"}),
            ),
            0,
        );
        assert!(store.incoming_call().is_none(), "it has been answered");
        assert!(store.my_call().is_some());
    }

    #[test]
    fn a_blocked_members_messages_are_hidden_from_a_channel() {
        // The server cannot drop a channel message for one reader, so
        // blocking has to be honoured here or it does nothing in channels.
        let mut store = store_with_me("me");
        store.apply_event(&frame("MESSAGE_CREATE", message("1", "general", "ada")), 0);
        store.apply_event(&frame("MESSAGE_CREATE", message("2", "general", "spam")), 0);
        assert_eq!(store.visible_messages_in("general").len(), 2);

        store.set_relationship("spam", RelationshipKind::Blocked);
        let visible = store.visible_messages_in("general");
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, "1");
        // The raw list is untouched, so a reply to a hidden message still
        // resolves its author.
        assert_eq!(store.messages_in("general").len(), 2);
    }

    #[test]
    fn a_relationship_answers_immediately_and_can_be_cleared() {
        let mut store = store_with_me("me");
        assert_eq!(store.relationship("ada"), RelationshipKind::None);

        store.set_relationship("ada", RelationshipKind::Outgoing);
        assert_eq!(store.relationship("ada"), RelationshipKind::Outgoing);

        store.set_relationship("ada", RelationshipKind::Friend);
        assert_eq!(store.relationship("ada"), RelationshipKind::Friend);
        assert_eq!(
            store.relationships.len(),
            1,
            "the row is updated, not added"
        );

        // Unfriending with no favourite leaves no row behind.
        store.set_relationship("ada", RelationshipKind::None);
        assert!(store.relationships.is_empty());
    }

    #[test]
    fn a_favourite_survives_unfriending() {
        // Favourites are private and independent of friendship; losing one
        // when a friendship ends would quietly discard the member's own
        // bookmark.
        let mut store = store_with_me("me");
        store.set_relationship("ada", RelationshipKind::Friend);
        store.set_favourite("ada", true);
        store.set_relationship("ada", RelationshipKind::None);

        assert!(store.is_favourite("ada"));
        assert_eq!(store.relationship("ada"), RelationshipKind::None);
    }

    #[test]
    fn blocking_is_visible_and_counts_incoming_requests() {
        let mut store = store_with_me("me");
        store.set_relationship("spam", RelationshipKind::Blocked);
        assert!(store.is_blocked("spam"));

        store.set_relationship("ada", RelationshipKind::Incoming);
        store.set_relationship("grace", RelationshipKind::Incoming);
        store.set_relationship("alan", RelationshipKind::Friend);
        assert_eq!(store.incoming_requests(), 2);
    }

    #[test]
    fn an_unknown_event_changes_nothing() {
        let mut store = store_with_me("me");
        assert!(!store.apply_event(&frame("INVENTED_LATER", json!({})), 0));
    }

    #[test]
    fn a_malformed_payload_is_ignored_rather_than_panicking() {
        let mut store = store_with_me("me");
        assert!(!store.apply_event(&frame("MESSAGE_CREATE", json!("not an object")), 0));
        assert!(!store.apply_event(&frame("MEMBER_UPDATE", json!({ "no": "id" })), 0));
        assert!(store.messages.is_empty());
    }

    #[test]
    fn categories_collapse_and_expand() {
        let mut store = store_with_me("me");
        store.toggle_category("c1");
        assert!(store.collapsed_categories.contains(&"c1".to_string()));
        store.toggle_category("c1");
        assert!(store.collapsed_categories.is_empty());
    }

    #[test]
    fn paging_back_keeps_the_older_messages_it_just_fetched() {
        let mut store = store_with_me("me");
        let newer: Vec<Message> = (100..200)
            .map(|i| Message {
                id: i.to_string(),
                created_at: format!("2026-03-04 11:{:02}:00", i % 60),
                ..Default::default()
            })
            .collect();
        store.set_messages("general", newer);

        let older: Vec<Message> = (0..100)
            .map(|i| Message {
                id: format!("old{i}"),
                created_at: format!("2026-03-04 09:{:02}:00", i % 60),
                ..Default::default()
            })
            .collect();
        store.prepend_messages("general", older);

        let list = store.messages_in("general");
        assert!(
            list.iter().any(|m| m.id == "old0"),
            "the fetched page was dropped"
        );
        assert!(list.len() <= MAX_MESSAGES_PER_CHANNEL);
    }
}

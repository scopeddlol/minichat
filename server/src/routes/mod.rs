pub mod admin;
pub mod auth_routes;
pub mod channels;
pub mod instance;
pub mod invites;
pub mod messages;
pub mod setup;
pub mod snapshot;
pub mod uploads;
pub mod users;
pub mod voice;
pub mod webhooks;

use axum::routing::{delete, get, patch, post, put};
use axum::Router;

use crate::gateway;
use crate::state::AppState;

pub fn api_router() -> Router<AppState> {
    Router::new()
        // ---- public / unauthenticated ----
        .route("/meta", get(instance::meta))
        .route("/setup", post(setup::run_setup))
        .route("/auth/register", post(auth_routes::register))
        .route("/auth/login", post(auth_routes::login))
        .route("/auth/me", get(auth_routes::me))
        .route("/auth/password", post(auth_routes::change_password))
        .route("/auth/revoke-sessions", post(auth_routes::revoke_sessions))
        .route("/invites/preview/{code}", get(invites::preview))
        .route("/gateway", get(gateway::handler))
        // ---- users ----
        .route("/users/@me", patch(users::update_me))
        .route("/users/{id}", get(users::get_user))
        .route("/members", get(users::list_members))
        // ---- channels ----
        .route(
            "/channels",
            get(channels::list_channels).post(channels::create_channel),
        )
        .route(
            "/channels/{id}",
            patch(channels::update_channel).delete(channels::delete_channel),
        )
        .route("/channels/reorder", post(channels::reorder))
        .route(
            "/channels/permissions",
            get(channels::my_channel_permissions),
        )
        .route(
            "/channels/{id}/overwrites",
            get(channels::list_overwrites).put(channels::set_overwrite),
        )
        .route(
            "/categories",
            get(channels::list_categories).post(channels::create_category),
        )
        .route(
            "/categories/{id}",
            patch(channels::update_category).delete(channels::delete_category),
        )
        // ---- messages ----
        .route(
            "/channels/{id}/messages",
            get(messages::list_messages).post(messages::create_message),
        )
        .route("/channels/{id}/pins", get(messages::list_pins))
        .route("/channels/{id}/ack", post(messages::ack))
        .route(
            "/messages/{id}",
            patch(messages::edit_message).delete(messages::delete_message),
        )
        .route(
            "/messages/{id}/pin",
            put(messages::pin).delete(messages::unpin),
        )
        .route(
            "/messages/{id}/reactions/{emoji}",
            put(messages::add_reaction).delete(messages::remove_reaction),
        )
        .route("/search", get(messages::search))
        // ---- uploads ----
        .route("/uploads", post(uploads::upload))
        // ---- voice ----
        .route("/voice/{channel_id}/token", post(voice::token))
        .route("/voice/states", get(voice::states))
        .route("/voice/disconnect/{user_id}", post(voice::force_disconnect))
        // ---- invites ----
        .route(
            "/invites",
            get(invites::list_invites).post(invites::create_invite),
        )
        .route("/invites/{code}", delete(invites::revoke_invite))
        // ---- admin ----
        .route(
            "/admin/instance",
            get(admin::get_instance).patch(admin::update_instance),
        )
        .route("/admin/stats", get(admin::stats))
        .route("/admin/permissions", get(admin::permission_catalog))
        .route(
            "/admin/roles",
            get(admin::list_roles).post(admin::create_role),
        )
        .route(
            "/admin/roles/{id}",
            patch(admin::update_role).delete(admin::delete_role),
        )
        .route(
            "/admin/members/{id}/roles/{role_id}",
            put(admin::add_member_role).delete(admin::remove_member_role),
        )
        .route(
            "/admin/members/{id}",
            patch(admin::update_member).delete(admin::kick_member),
        )
        .route("/admin/bans", get(admin::list_bans))
        .route(
            "/admin/bans/{id}",
            put(admin::ban_member).delete(admin::unban_member),
        )
        .route("/admin/audit", get(admin::audit_log))
        .route(
            "/admin/webhooks",
            get(webhooks::list_webhooks).post(webhooks::create_webhook),
        )
        .route("/admin/webhooks/{id}", delete(webhooks::delete_webhook))
}

/// Webhook execution lives outside `/api` auth: it authenticates with the
/// webhook's own token in the path.
pub fn webhook_router() -> Router<AppState> {
    Router::new().route("/{id}/{token}", post(webhooks::execute))
}

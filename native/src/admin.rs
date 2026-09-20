//! What the admin panel shows.
//!
//! Which sections a member sees follows their permissions, and which actions
//! appear on a row follows the role hierarchy as well. Both are worked out
//! here, as pure functions, because "can this member ban that one" is a rule
//! worth being able to test without a window — and because getting it wrong
//! shows an action the server will refuse.

use crate::api::types::{Member, Role};
use crate::perms;

/// A section of the panel.
pub struct Section {
    pub id: &'static str,
    pub label: &'static str,
    /// The permission that reveals it. `0` means everyone who can open the
    /// panel at all.
    pub required: u64,
}

pub const SECTIONS: &[Section] = &[
    Section {
        id: "overview",
        label: "Overview",
        required: 0,
    },
    Section {
        id: "instance",
        label: "Instance",
        required: perms::MANAGE_INSTANCE,
    },
    Section {
        id: "members",
        label: "Members",
        required: perms::KICK_MEMBERS,
    },
    Section {
        id: "roles",
        label: "Roles",
        required: perms::MANAGE_ROLES,
    },
    Section {
        id: "invites",
        label: "Invites",
        required: perms::CREATE_INVITES,
    },
    Section {
        id: "bans",
        label: "Bans",
        required: perms::BAN_MEMBERS,
    },
    Section {
        id: "audit",
        label: "Audit log",
        required: perms::VIEW_AUDIT_LOG,
    },
];

/// The sections this member may see.
pub fn sections_for(bits: u64) -> Vec<&'static Section> {
    SECTIONS
        .iter()
        .filter(|section| section.required == 0 || perms::can(bits, section.required))
        .collect()
}

/// A member's rank: the position of their highest role.
///
/// Someone with no roles ranks below everyone with one, which is why this is
/// `-1` rather than `0` — a role at position 0 still outranks no role at all.
pub fn rank(member: &Member, roles: &[Role]) -> i64 {
    roles
        .iter()
        .filter(|role| member.roles.iter().any(|id| id == &role.id))
        .map(|role| role.position)
        .max()
        .unwrap_or(-1)
}

/// Whether `actor` may act on `target` at all.
///
/// The rule the server applies: you may not touch yourself, the operator, or
/// anyone who outranks you or matches your rank. An operator is exempt from
/// the rank part but still cannot act on themselves.
pub fn outranks(actor: &Member, target: &Member, roles: &[Role]) -> bool {
    if actor.id == target.id {
        return false;
    }
    // The operator account can never be kicked or banned — it is the one
    // that cannot be locked out.
    if target.is_operator {
        return false;
    }
    if actor.is_operator {
        return true;
    }
    rank(actor, roles) > rank(target, roles)
}

/// Whether `actor` may kick `target`.
pub fn can_kick(actor: &Member, target: &Member, roles: &[Role], bits: u64) -> bool {
    perms::can(bits, perms::KICK_MEMBERS) && outranks(actor, target, roles)
}

pub fn can_ban(actor: &Member, target: &Member, roles: &[Role], bits: u64) -> bool {
    perms::can(bits, perms::BAN_MEMBERS) && outranks(actor, target, roles)
}

/// Whether `actor` may grant or remove `role`.
///
/// You cannot hand out a role at or above your own rank; otherwise every
/// moderator could promote themselves.
pub fn can_assign(actor: &Member, role: &Role, roles: &[Role], bits: u64) -> bool {
    if !perms::can(bits, perms::MANAGE_ROLES) {
        return false;
    }
    if actor.is_operator {
        return true;
    }
    role.position < rank(actor, roles)
}

/// A member's roles as a line of text, highest first.
pub fn role_summary(member: &Member, roles: &[Role]) -> String {
    let mut theirs: Vec<&Role> = roles
        .iter()
        .filter(|role| member.roles.iter().any(|id| id == &role.id))
        .filter(|role| !role.is_default)
        .collect();
    theirs.sort_by_key(|role| -role.position);
    theirs
        .iter()
        .map(|role| role.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// How an invite's usage reads.
pub fn invite_uses(uses: i64, max_uses: i64) -> String {
    if max_uses <= 0 {
        return format!("{uses} used · unlimited");
    }
    format!("{uses} of {max_uses} used")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str, roles: &[&str], operator: bool) -> Member {
        Member {
            id: id.into(),
            username: id.into(),
            display_name: id.into(),
            roles: roles.iter().map(|r| r.to_string()).collect(),
            is_operator: operator,
            ..Default::default()
        }
    }

    fn role(id: &str, position: i64) -> Role {
        Role {
            id: id.into(),
            name: id.into(),
            position,
            ..Default::default()
        }
    }

    fn hierarchy() -> Vec<Role> {
        vec![role("admin", 10), role("mod", 5), role("member", 0)]
    }

    #[test]
    fn only_the_sections_a_member_can_use_appear() {
        let ids = |bits| -> Vec<&str> { sections_for(bits).iter().map(|s| s.id).collect() };

        // Someone who can only see the audit log gets that and the overview.
        assert_eq!(ids(perms::VIEW_AUDIT_LOG), ["overview", "audit"]);
        // An administrator gets everything.
        assert_eq!(ids(perms::ADMINISTRATOR).len(), SECTIONS.len());
    }

    #[test]
    fn rank_comes_from_the_highest_role_and_no_roles_ranks_lowest() {
        let roles = hierarchy();
        assert_eq!(rank(&member("a", &["mod", "member"], false), &roles), 5);
        // A role at position 0 still beats having none, so no roles is -1.
        assert_eq!(rank(&member("b", &["member"], false), &roles), 0);
        assert_eq!(rank(&member("c", &[], false), &roles), -1);
    }

    #[test]
    fn you_cannot_act_on_yourself_or_on_the_operator() {
        let roles = hierarchy();
        let admin = member("admin", &["admin"], false);
        let operator = member("op", &[], true);

        assert!(!outranks(&admin, &admin, &roles), "not on yourself");
        assert!(
            !outranks(&admin, &operator, &roles),
            "the operator is the account that cannot be locked out"
        );
    }

    #[test]
    fn an_equal_rank_does_not_outrank() {
        // Two moderators cannot kick each other, which is the whole point of
        // a hierarchy that compares rather than permits.
        let roles = hierarchy();
        let one = member("one", &["mod"], false);
        let two = member("two", &["mod"], false);
        assert!(!outranks(&one, &two, &roles));
        assert!(!outranks(&two, &one, &roles));
    }

    #[test]
    fn the_operator_outranks_everyone_but_themselves() {
        let roles = hierarchy();
        let operator = member("op", &[], true);
        let admin = member("admin", &["admin"], false);
        assert!(outranks(&operator, &admin, &roles));
        assert!(!outranks(&operator, &operator, &roles));
    }

    #[test]
    fn kicking_needs_both_the_permission_and_the_rank() {
        let roles = hierarchy();
        let moderator = member("mod", &["mod"], false);
        let ordinary = member("member", &["member"], false);
        let admin = member("admin", &["admin"], false);

        assert!(can_kick(&moderator, &ordinary, &roles, perms::KICK_MEMBERS));
        // The rank is there but the permission is not.
        assert!(!can_kick(&moderator, &ordinary, &roles, 0));
        // The permission is there but the rank is not.
        assert!(!can_kick(&moderator, &admin, &roles, perms::KICK_MEMBERS));
    }

    #[test]
    fn nobody_may_hand_out_a_role_at_or_above_their_own_rank() {
        // Otherwise every moderator promotes themselves.
        let roles = hierarchy();
        let moderator = member("mod", &["mod"], false);
        let bits = perms::MANAGE_ROLES;

        assert!(can_assign(&moderator, &role("member", 0), &roles, bits));
        assert!(!can_assign(&moderator, &role("mod", 5), &roles, bits));
        assert!(!can_assign(&moderator, &role("admin", 10), &roles, bits));

        // The operator is exempt from the rank rule.
        let operator = member("op", &[], true);
        assert!(can_assign(&operator, &role("admin", 10), &roles, bits));
        // But not from needing the permission.
        assert!(!can_assign(&operator, &role("admin", 10), &roles, 0));
    }

    #[test]
    fn the_role_summary_is_highest_first_and_skips_the_default() {
        let mut roles = hierarchy();
        roles.push(Role {
            id: "everyone".into(),
            name: "everyone".into(),
            position: -1,
            is_default: true,
            ..Default::default()
        });
        let person = member("a", &["member", "admin", "everyone"], false);
        assert_eq!(role_summary(&person, &roles), "admin, member");
    }

    #[test]
    fn invite_usage_reads_sensibly_for_both_kinds() {
        assert_eq!(invite_uses(3, 10), "3 of 10 used");
        assert_eq!(invite_uses(3, 0), "3 used · unlimited");
    }
}

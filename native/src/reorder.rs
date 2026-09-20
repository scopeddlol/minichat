//! Moving a channel in the sidebar.
//!
//! The server takes a complete order, not a delta (`POST /channels/reorder`),
//! so the whole list is recomputed here and sent. Doing it that way means the
//! server never has to reconcile a partial move, and it means this is a pure
//! function over the channel list — which is the only reason it can be tested
//! without a window.

use crate::api::types::{Category, Channel};

/// One entry of the order to send: the channel, its new position, and the
/// category it now belongs to.
pub type Placement = (String, i64, Option<String>);

/// The sidebar's channels in the order they are drawn.
///
/// Uncategorised first, then each category's channels in category order —
/// the same walk the sidebar does, so an index here is a row there.
pub fn visible_order(channels: &[Channel], categories: &[Category]) -> Vec<Channel> {
    let mut out: Vec<Channel> = channels
        .iter()
        .filter(|c| c.category_id.is_none())
        .cloned()
        .collect();
    out.sort_by_key(|c| c.position);

    let mut ordered_categories: Vec<&Category> = categories.iter().collect();
    ordered_categories.sort_by_key(|c| c.position);

    for category in ordered_categories {
        let mut inside: Vec<Channel> = channels
            .iter()
            .filter(|c| c.category_id.as_deref() == Some(category.id.as_str()))
            .cloned()
            .collect();
        inside.sort_by_key(|c| c.position);
        out.extend(inside);
    }
    out
}

/// Move the channel at `from` to `to`, and return the order to send.
///
/// A channel takes the category of wherever it lands, which is what makes
/// dragging into another category work without a separate gesture.
pub fn move_channel(
    channels: &[Channel],
    categories: &[Category],
    channel_id: &str,
    offset: i32,
) -> Option<Vec<Placement>> {
    let order = visible_order(channels, categories);
    let from = order.iter().position(|c| c.id == channel_id)?;

    // Clamped rather than wrapped: dragging past the end should stop at the
    // end, not reappear at the top.
    let to = (from as i32 + offset).clamp(0, order.len().saturating_sub(1) as i32) as usize;
    if to == from {
        return None;
    }

    let mut moved: Vec<Channel> = order.clone();
    let channel = moved.remove(from);
    moved.insert(to, channel);

    Some(placements(&moved, categories, channel_id, to))
}

/// Assign every channel its category and position from the moved list.
fn placements(
    moved: &[Channel],
    categories: &[Category],
    channel_id: &str,
    landed_at: usize,
) -> Vec<Placement> {
    // The category a moved channel adopts is the one its neighbours are in.
    // The neighbour *above* decides: dropping just below the last channel of
    // a category reads as joining it, not as starting the next one.
    let adopted = neighbour_category(moved, landed_at);

    let mut out = Vec::with_capacity(moved.len());
    let mut position_in: std::collections::HashMap<Option<String>, i64> = Default::default();

    for (index, channel) in moved.iter().enumerate() {
        let category = if channel.id == channel_id {
            adopted.clone()
        } else {
            channel.category_id.clone()
        };
        // Positions restart in each category, which is how the sidebar reads
        // them back.
        let slot = position_in.entry(category.clone()).or_insert(0);
        out.push((channel.id.clone(), *slot, category));
        *slot += 1;
        let _ = index;
    }
    let _ = categories;
    out
}

/// The category the row above belongs to, falling back to the row below and
/// then to no category at all.
fn neighbour_category(moved: &[Channel], at: usize) -> Option<String> {
    if at > 0 {
        if let Some(above) = moved.get(at - 1) {
            return above.category_id.clone();
        }
    }
    moved
        .get(at + 1)
        .and_then(|below| below.category_id.clone())
}

/// Move a whole category up or down, and return the new category order.
pub fn move_category(
    categories: &[Category],
    category_id: &str,
    offset: i32,
) -> Option<Vec<(String, i64)>> {
    let mut ordered: Vec<&Category> = categories.iter().collect();
    ordered.sort_by_key(|c| c.position);

    let from = ordered.iter().position(|c| c.id == category_id)?;
    let to = (from as i32 + offset).clamp(0, ordered.len().saturating_sub(1) as i32) as usize;
    if to == from {
        return None;
    }

    let moved = ordered.remove(from);
    ordered.insert(to, moved);

    Some(
        ordered
            .iter()
            .enumerate()
            .map(|(index, category)| (category.id.clone(), index as i64))
            .collect(),
    )
}

/// How many rows a drag of `distance` pixels covers.
///
/// Channel rows are a uniform height, so a drag maps to a whole number of
/// rows rather than to a pixel offset that then has to be resolved against
/// every row's position.
pub fn rows_moved(distance: f32, row_height: f32) -> i32 {
    if row_height <= 0.0 {
        return 0;
    }
    (distance / row_height).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(id: &str, position: i64, category: Option<&str>) -> Channel {
        Channel {
            id: id.into(),
            name: id.into(),
            position,
            category_id: category.map(str::to_string),
            ..Default::default()
        }
    }

    fn category(id: &str, position: i64) -> Category {
        Category {
            id: id.into(),
            name: id.into(),
            position,
            is_private: false,
        }
    }

    fn fixture() -> (Vec<Channel>, Vec<Category>) {
        // welcome (loose), then TEXT: general, design, then VOICE: lounge.
        let channels = vec![
            channel("welcome", 0, None),
            channel("general", 0, Some("text")),
            channel("design", 1, Some("text")),
            channel("lounge", 0, Some("voice")),
        ];
        let categories = vec![category("text", 0), category("voice", 1)];
        (channels, categories)
    }

    #[test]
    fn the_visible_order_is_loose_channels_then_each_category() {
        let (channels, categories) = fixture();
        let names: Vec<String> = visible_order(&channels, &categories)
            .iter()
            .map(|c| c.id.clone())
            .collect();
        assert_eq!(names, ["welcome", "general", "design", "lounge"]);
    }

    #[test]
    fn moving_a_channel_down_one_swaps_it_with_its_neighbour() {
        let (channels, categories) = fixture();
        let order = move_channel(&channels, &categories, "general", 1).unwrap();
        let ids: Vec<&str> = order.iter().map(|(id, _, _)| id.as_str()).collect();
        assert_eq!(ids, ["welcome", "design", "general", "lounge"]);
        // Both are still in the text category, renumbered from zero.
        let general = order.iter().find(|(id, _, _)| id == "general").unwrap();
        assert_eq!(general.2.as_deref(), Some("text"));
        assert_eq!(general.1, 1);
    }

    #[test]
    fn a_channel_dragged_into_another_category_adopts_it() {
        // This is the whole point of dragging across the boundary: there is
        // no separate "move to category" gesture.
        let (channels, categories) = fixture();
        let order = move_channel(&channels, &categories, "design", 1).unwrap();
        let design = order.iter().find(|(id, _, _)| id == "design").unwrap();
        assert_eq!(
            design.2.as_deref(),
            Some("voice"),
            "landing under lounge should join the voice category"
        );
    }

    #[test]
    fn a_channel_dragged_to_the_top_becomes_uncategorised() {
        let (channels, categories) = fixture();
        let order = move_channel(&channels, &categories, "design", -3).unwrap();
        let design = order.iter().find(|(id, _, _)| id == "design").unwrap();
        assert_eq!(design.1, 0);
        assert_eq!(design.2, None, "above every category means no category");
    }

    #[test]
    fn dragging_past_the_end_stops_at_the_end_rather_than_wrapping() {
        let (channels, categories) = fixture();
        let order = move_channel(&channels, &categories, "welcome", 99).unwrap();
        let ids: Vec<&str> = order.iter().map(|(id, _, _)| id.as_str()).collect();
        assert_eq!(ids.last(), Some(&"welcome"));

        let order = move_channel(&channels, &categories, "lounge", -99).unwrap();
        let ids: Vec<&str> = order.iter().map(|(id, _, _)| id.as_str()).collect();
        assert_eq!(ids.first(), Some(&"lounge"));
    }

    #[test]
    fn a_move_that_changes_nothing_sends_nothing() {
        let (channels, categories) = fixture();
        assert!(move_channel(&channels, &categories, "general", 0).is_none());
        // Already at the top, dragged further up.
        assert!(move_channel(&channels, &categories, "welcome", -5).is_none());
        // An unknown channel is not a move.
        assert!(move_channel(&channels, &categories, "nope", 1).is_none());
    }

    #[test]
    fn positions_restart_within_each_category() {
        let (channels, categories) = fixture();
        let order = move_channel(&channels, &categories, "general", 1).unwrap();
        let mut seen: std::collections::HashMap<Option<String>, Vec<i64>> = Default::default();
        for (_, position, category) in &order {
            seen.entry(category.clone()).or_default().push(*position);
        }
        for (category, positions) in seen {
            let expected: Vec<i64> = (0..positions.len() as i64).collect();
            assert_eq!(
                positions, expected,
                "positions in {category:?} are not 0..n"
            );
        }
    }

    #[test]
    fn categories_move_and_renumber() {
        let categories = vec![category("a", 0), category("b", 1), category("c", 2)];
        let order = move_category(&categories, "c", -2).unwrap();
        assert_eq!(
            order,
            vec![("c".into(), 0), ("a".into(), 1), ("b".into(), 2)]
        );
        assert!(move_category(&categories, "a", -1).is_none());
        assert!(move_category(&categories, "missing", 1).is_none());
    }

    #[test]
    fn a_drag_maps_to_whole_rows() {
        assert_eq!(rows_moved(0.0, 38.0), 0);
        assert_eq!(rows_moved(20.0, 38.0), 1);
        assert_eq!(rows_moved(-45.0, 38.0), -1);
        assert_eq!(rows_moved(80.0, 38.0), 2);
        // A degenerate row height must not divide by zero.
        assert_eq!(rows_moved(100.0, 0.0), 0);
    }
}

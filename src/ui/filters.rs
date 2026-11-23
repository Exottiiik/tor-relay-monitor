//! Bounded, in-memory views over collected events and OR neighbors.
use crate::model::{LogEntry, State};

pub struct LogGroup<'a> {
    pub entry: &'a LogEntry,
    pub count: usize,
    pub first: u64,
}

pub fn logs<'a>(state: &'a State, query: &str, level: u8, group: bool) -> Vec<LogGroup<'a>> {
    let mut groups: Vec<LogGroup<'a>> = Vec::new();
    // Group before filtering: hidden intervening events must not merge runs.
    for entry in &state.logs {
        if group
            && let Some(last) = groups.last_mut()
            && last.entry.level == entry.level
            && last.entry.message == entry.message
        {
            last.entry = entry;
            last.count += 1;
            continue;
        }
        groups.push(LogGroup {
            entry,
            count: 1,
            first: entry.seconds,
        });
    }
    let query = query.to_lowercase();
    groups
        .into_iter()
        .rev()
        .filter(|g| {
            let severity = match level {
                1 => matches!(g.entry.level.as_str(), "WARN" | "ERR"),
                2 => g.entry.level == "ERR",
                _ => true,
            };
            severity
                && (query.is_empty()
                    || g.entry.message.to_lowercase().contains(&query)
                    || g.entry.level.to_lowercase().contains(&query))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Status,
    Name,
    Identity,
}
impl Sort {
    pub fn next(self) -> Self {
        match self {
            Self::Status => Self::Name,
            Self::Name => Self::Identity,
            Self::Identity => Self::Status,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Name => "name",
            Self::Identity => "identity",
        }
    }
}

pub struct Neighbor<'a> {
    pub peer: &'a str,
    pub status: &'a str,
    pub name: String,
    pub identity: String,
}

pub fn connections<'a>(
    state: &'a State,
    query: &str,
    sort: Sort,
    reverse: bool,
) -> Vec<Neighbor<'a>> {
    let query = query.to_lowercase();
    let mut rows: Vec<_> = state
        .connections
        .iter()
        .filter(|(peer, status)| {
            query.is_empty()
                || peer.to_lowercase().contains(&query)
                || status.to_lowercase().contains(&query)
        })
        .map(|(peer, status)| {
            let (name, identity) = super::peer_parts(peer);
            Neighbor {
                peer,
                status,
                name,
                identity,
            }
        })
        .collect();
    // Cache normalized keys once; comparison never reparses or allocates.
    rows.sort_by_cached_key(|row| {
        let primary = match sort {
            Sort::Status => row.status.to_lowercase(),
            Sort::Name => row.name.to_lowercase(),
            Sort::Identity => row.identity.to_lowercase(),
        };
        (
            primary,
            row.identity.to_lowercase(),
            row.peer.to_lowercase(),
        )
    });
    if reverse {
        rows.reverse();
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filtering_preserves_separated_repetitions_and_severity() {
        let mut s = State::default();
        for (l, m) in [
            ("WARN", "Same"),
            ("WARN", "Same"),
            ("NOTICE", "Other"),
            ("WARN", "Same"),
            ("ERR", "Failure"),
        ] {
            s.log(l, m);
        }
        let rows = logs(&s, "sAmE", 1, true);
        assert_eq!(rows.iter().map(|g| g.count).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(logs(&s, "", 2, true).len(), 1);
        assert_eq!(logs(&s, "", 0, false).len(), 5);
        assert!(logs(&s, "missing", 0, true).is_empty());
    }
    #[test]
    fn neighbors_search_names_identities_and_states_then_sort_stably() {
        let mut s = State::default();
        s.or_connections(Some(
            "$BBB=zeta CONNECTED\n$AAA=Alpha LAUNCHED\n$CCC=alpha CONNECTED",
        ));
        let rows = connections(&s, "ALPHA", Sort::Name, false);
        assert_eq!(
            rows.iter().map(|r| r.identity.as_str()).collect::<Vec<_>>(),
            ["AAA", "CCC"]
        );
        assert_eq!(connections(&s, "bbb", Sort::Identity, true)[0].name, "zeta");
        assert_eq!(connections(&s, "connected", Sort::Status, false).len(), 2);
        assert_eq!(connections(&s, "", Sort::Identity, true)[0].identity, "CCC");
    }
}

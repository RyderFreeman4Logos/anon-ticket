//! Domain-level building blocks shared across API and monitor crates.
//!
//! The current placeholder focuses on proving that the multi-crate workspace
//! is wired up correctly. Future commits will replace this with rich Monero
//! payment and token primitives.

/// Returns a static readiness message so sibling crates can share a single
/// source of truth when reporting the workspace bootstrap status.
///
/// # Examples
/// ```
/// use anon_ticket_domain::workspace_ready_message;
/// assert!(workspace_ready_message().contains("workspace"));
/// ```
pub fn workspace_ready_message() -> &'static str {
    "anon-ticket workspace scaffolding ready"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_message_is_stable() {
        assert_eq!(
            workspace_ready_message(),
            "anon-ticket workspace scaffolding ready"
        );
    }
}

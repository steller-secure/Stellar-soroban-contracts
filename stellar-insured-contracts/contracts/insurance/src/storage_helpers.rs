/// Compact, namespaced storage key prefixes.
///
/// Short prefixes reduce per-entry overhead; grouping related keys under a
/// shared prefix makes access patterns explicit and avoids collisions.
pub mod keys {
    /// Prefix for all policy entries: `(POLICY_PREFIX, policy_id)`.
    pub const POLICY_PREFIX: &str = "pol";

    /// Prefix for all claim entries: `(CLAIM_PREFIX, claim_id)`.
    pub const CLAIM_PREFIX: &str = "clm";

    /// Prefix for risk-pool entries: `(POOL_PREFIX, pool_id)`.
    pub const POOL_PREFIX: &str = "rp";

    /// Singleton key for the admin address.
    pub const ADMIN_KEY: &str = "adm";

    /// Singleton key for the paused flag.
    pub const PAUSED_KEY: &str = "psd";

    /// Prefix for per-account policy index: `(ACCOUNT_PREFIX, account_id)`.
    pub const ACCOUNT_PREFIX: &str = "acc";
}

/// Returns `true` when `new_value` differs from `current`, avoiding a redundant write.
///
/// Call this before any `storage.set(key, value)` to skip writes whose value
/// hasn't changed — storage writes are metered and should be minimised.
pub fn needs_write<T: PartialEq>(current: &T, new_value: &T) -> bool {
    current != new_value
}

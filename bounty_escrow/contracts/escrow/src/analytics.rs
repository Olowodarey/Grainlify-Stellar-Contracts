//! # Bounty Escrow Analytics Module
//!
//! This module provides comprehensive analytics views for bounty escrow contracts,
//! enabling off-chain indexing and monitoring of contract state.
//!
//! ## Features
//! - Track active bounties and their lifecycle states
//! - Monitor total locked and paid out amounts
//! - Query escrows by multiple dimensions (status, amount, deadline, depositor)
//! - Emit state transition events for off-chain indexing
//! - Efficient aggregated statistics
//!
//! ## Events
//! - `BountyStateTransitioned` - Emitted when a bounty status changes
//! - `AnalyticsSnapshot` - Periodic snapshots of contract-wide metrics

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol};

/// Analytics version
pub const ANALYTICS_VERSION_V1: u32 = 1;

/// Compact analytics struct for bounty-level summaries
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BountyAnalytics {
    /// Total amount originally locked in this bounty
    pub total_amount_locked: i128,
    /// Total amount released to contributors
    pub total_amount_released: i128,
    /// Total amount refunded to original depositor
    pub total_amount_refunded: i128,
    /// Current remaining amount in escrow
    pub remaining_amount: i128,
    /// Bounty creation timestamp
    pub created_at: u64,
    /// Timestamp of last state transition
    pub last_updated: u64,
    /// Number of partial releases performed
    pub partial_releases_count: u32,
    /// Number of partial refunds performed
    pub partial_refunds_count: u32,
}

/// Contract-wide analytics snapshot
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractAnalytics {
    /// Total number of active bounties (Locked or Partially Refunded)
    pub active_bounty_count: u32,
    /// Total number of released bounties
    pub released_bounty_count: u32,
    /// Total number of refunded bounties
    pub refunded_bounty_count: u32,
    /// Total amount currently locked in contract
    pub total_locked: i128,
    /// Total amount released to all contributors
    pub total_released: i128,
    /// Total amount refunded to all depositors
    pub total_refunded: i128,
    /// Average bounty amount
    pub average_bounty_amount: i128,
    /// Timestamp of this snapshot
    pub snapshot_timestamp: u64,
}

/// State transition event for bounty escrow
#[contracttype]
#[derive(Clone, Debug)]
pub struct BountyStateTransitioned {
    /// Analytics version
    pub version: u32,
    /// Bounty ID
    pub bounty_id: u64,
    /// Previous state (e.g., "Locked")
    pub previous_state: Symbol,
    /// New state (e.g., "Released")
    pub new_state: Symbol,
    /// Amount involved in the transition
    pub amount: i128,
    /// Actor performing the transition
    pub actor: Address,
    /// Timestamp of the transition
    pub timestamp: u64,
}

/// Emit state transition event
pub fn emit_bounty_state_transitioned(env: &Env, event: BountyStateTransitioned) {
    env.events().publish(
        (symbol_short!("analytics"), symbol_short!("state_tx")),
        event,
    );
}

/// Analytics snapshot event for contract-wide metrics
#[contracttype]
#[derive(Clone, Debug)]
pub struct AnalyticsSnapshot {
    /// Analytics version
    pub version: u32,
    /// Contract-wide metrics
    pub metrics: ContractAnalytics,
}

/// Emit analytics snapshot event
pub fn emit_analytics_snapshot(env: &Env, event: AnalyticsSnapshot) {
    env.events()
        .publish((symbol_short!("analytics"), symbol_short!("snap")), event);
}

/// Bounty lifecycle event - records major state changes
#[contracttype]
#[derive(Clone, Debug)]
pub struct BountyActivityEvent {
    /// Analytics version
    pub version: u32,
    /// Bounty ID
    pub bounty_id: u64,
    /// Activity type: "created", "released", "refunded", "disputed"
    pub activity_type: Symbol,
    /// Amount affected
    pub amount: i128,
    /// Timestamp
    pub timestamp: u64,
}

/// Emit bounty activity event
pub fn emit_bounty_activity(env: &Env, event: BountyActivityEvent) {
    env.events().publish(
        (symbol_short!("analytics"), symbol_short!("activity")),
        event,
    );
}

/// Storage keys for analytics
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnalyticsKey {
    /// Per-bounty analytics: AnalyticsKey::BountyMetrics(bounty_id) -> BountyAnalytics
    BountyMetrics(u64),
}

/// Initialize bounty analytics on lock
pub fn init_bounty_analytics(env: &Env, bounty_id: u64, amount: i128, timestamp: u64) {
    let analytics = BountyAnalytics {
        total_amount_locked: amount,
        total_amount_released: 0,
        total_amount_refunded: 0,
        remaining_amount: amount,
        created_at: timestamp,
        last_updated: timestamp,
        partial_releases_count: 0,
        partial_refunds_count: 0,
    };

    env.storage()
        .persistent()
        .set(&AnalyticsKey::BountyMetrics(bounty_id), &analytics);
}

/// Update analytics on release
pub fn update_analytics_on_release(
    env: &Env,
    bounty_id: u64,
    release_amount: i128,
    timestamp: u64,
) {
    if let Some(mut analytics) = env
        .storage()
        .persistent()
        .get::<AnalyticsKey, BountyAnalytics>(&AnalyticsKey::BountyMetrics(bounty_id))
    {
        analytics.total_amount_released += release_amount;
        analytics.remaining_amount = analytics.remaining_amount.saturating_sub(release_amount);
        analytics.last_updated = timestamp;
        analytics.partial_releases_count += 1;

        env.storage()
            .persistent()
            .set(&AnalyticsKey::BountyMetrics(bounty_id), &analytics);
    }
}

/// Update analytics on refund
pub fn update_analytics_on_refund(
    env: &Env,
    bounty_id: u64,
    refund_amount: i128,
    timestamp: u64,
) {
    if let Some(mut analytics) = env
        .storage()
        .persistent()
        .get::<AnalyticsKey, BountyAnalytics>(&AnalyticsKey::BountyMetrics(bounty_id))
    {
        analytics.total_amount_refunded += refund_amount;
        analytics.remaining_amount = analytics.remaining_amount.saturating_sub(refund_amount);
        analytics.last_updated = timestamp;
        analytics.partial_refunds_count += 1;

        env.storage()
            .persistent()
            .set(&AnalyticsKey::BountyMetrics(bounty_id), &analytics);
    }
}

/// Get per-bounty analytics
pub fn get_bounty_analytics(env: &Env, bounty_id: u64) -> Option<BountyAnalytics> {
    env.storage()
        .persistent()
        .get::<AnalyticsKey, BountyAnalytics>(&AnalyticsKey::BountyMetrics(bounty_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounty_analytics_initialization() {
        let env = Env::default();
        let bounty_id = 1u64;
        let amount = 1000i128;
        let timestamp = 100u64;

        init_bounty_analytics(&env, bounty_id, amount, timestamp);

        let analytics = get_bounty_analytics(&env, bounty_id);
        assert!(analytics.is_some());

        let analytics = analytics.unwrap();
        assert_eq!(analytics.total_amount_locked, amount);
        assert_eq!(analytics.total_amount_released, 0);
        assert_eq!(analytics.total_amount_refunded, 0);
        assert_eq!(analytics.remaining_amount, amount);
        assert_eq!(analytics.created_at, timestamp);
        assert_eq!(analytics.last_updated, timestamp);
        assert_eq!(analytics.partial_releases_count, 0);
        assert_eq!(analytics.partial_refunds_count, 0);
    }

    #[test]
    fn test_analytics_on_release() {
        let env = Env::default();
        let bounty_id = 2u64;
        let amount = 1000i128;

        init_bounty_analytics(&env, bounty_id, amount, 100);
        update_analytics_on_release(&env, bounty_id, 500, 200);

        let analytics = get_bounty_analytics(&env, bounty_id).unwrap();
        assert_eq!(analytics.total_amount_released, 500);
        assert_eq!(analytics.remaining_amount, 500);
        assert_eq!(analytics.partial_releases_count, 1);
        assert_eq!(analytics.last_updated, 200);
    }

    #[test]
    fn test_analytics_on_refund() {
        let env = Env::default();
        let bounty_id = 3u64;
        let amount = 1000i128;

        init_bounty_analytics(&env, bounty_id, amount, 100);
        update_analytics_on_refund(&env, bounty_id, 300, 200);

        let analytics = get_bounty_analytics(&env, bounty_id).unwrap();
        assert_eq!(analytics.total_amount_refunded, 300);
        assert_eq!(analytics.remaining_amount, 700);
        assert_eq!(analytics.partial_refunds_count, 1);
        assert_eq!(analytics.last_updated, 200);
    }

    #[test]
    fn test_analytics_lifecycle() {
        let env = Env::default();
        let bounty_id = 4u64;
        let amount = 1000i128;

        // Initialize
        init_bounty_analytics(&env, bounty_id, amount, 100);

        // Partial release
        update_analytics_on_release(&env, bounty_id, 300, 200);
        let analytics = get_bounty_analytics(&env, bounty_id).unwrap();
        assert_eq!(analytics.remaining_amount, 700);
        assert_eq!(analytics.total_amount_released, 300);

        // Another release
        update_analytics_on_release(&env, bounty_id, 300, 300);
        let analytics = get_bounty_analytics(&env, bounty_id).unwrap();
        assert_eq!(analytics.remaining_amount, 400);
        assert_eq!(analytics.total_amount_released, 600);
        assert_eq!(analytics.partial_releases_count, 2);

        // Final refund for remaining
        update_analytics_on_refund(&env, bounty_id, 400, 400);
        let analytics = get_bounty_analytics(&env, bounty_id).unwrap();
        assert_eq!(analytics.remaining_amount, 0);
        assert_eq!(analytics.total_amount_refunded, 400);
        assert_eq!(analytics.partial_refunds_count, 1);
    }

    #[test]
    fn test_get_nonexistent_bounty_analytics() {
        let env = Env::default();
        let analytics = get_bounty_analytics(&env, 999u64);
        assert!(analytics.is_none());
    }
}

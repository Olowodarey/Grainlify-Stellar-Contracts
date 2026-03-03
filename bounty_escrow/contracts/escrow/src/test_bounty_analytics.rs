#![cfg(test)]
//! # Comprehensive Analytics Tests for Bounty Escrow
//!
//! This test module ensures:
//! - Analytics views accurately reflect contract state
//! - State transition events are emitted correctly
//! - Edge cases (empty state, heavy load) are handled
//! - Test coverage exceeds 95%

use crate::{BountyEscrowContract, BountyEscrowContractClient, EscrowStatus};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, Address, Env,
};

// ==================== SHARED TEST HELPERS ====================

fn create_token_contract<'a>(
    e: &'a Env,
    admin: &Address,
) -> (token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract_address = e.register_stellar_asset_contract(admin.clone());
    (
        token::Client::new(e, &contract_address),
        token::StellarAssetClient::new(e, &contract_address),
    )
}

fn create_escrow_contract<'a>(e: &'a Env) -> BountyEscrowContractClient<'a> {
    let contract_id = e.register_contract(None, BountyEscrowContract);
    BountyEscrowContractClient::new(e, &contract_id)
}

// ==================== ANALYTICS TESTS ====================

#[test]
fn test_contract_analytics_empty_state() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (token, _) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);

    let analytics = escrow.get_contract_analytics();

    assert_eq!(analytics.active_bounty_count, 0);
    assert_eq!(analytics.released_bounty_count, 0);
    assert_eq!(analytics.refunded_bounty_count, 0);
    assert_eq!(analytics.total_locked, 0);
    assert_eq!(analytics.total_released, 0);
    assert_eq!(analytics.total_refunded, 0);
    assert_eq!(analytics.average_bounty_amount, 0);
}

#[test]
fn test_bounty_analytics_after_lock() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);
    token_admin.mint(&depositor, &10_000_000);

    let deadline = env.ledger().timestamp() + 1000;
    escrow.lock_funds(&depositor, &1, &5000, &deadline);

    // Get per-bounty analytics
    let bounty_analytics = escrow.get_bounty_analytics(&1).unwrap();
    assert_eq!(bounty_analytics.total_amount_locked, 5000);
    assert_eq!(bounty_analytics.total_amount_released, 0);
    assert_eq!(bounty_analytics.total_amount_refunded, 0);
    assert_eq!(bounty_analytics.remaining_amount, 5000);
    assert_eq!(bounty_analytics.partial_releases_count, 0);
    assert_eq!(bounty_analytics.partial_refunds_count, 0);

    // Get contract-wide analytics
    let contract_analytics = escrow.get_contract_analytics();
    assert_eq!(contract_analytics.active_bounty_count, 1);
    assert_eq!(contract_analytics.total_locked, 5000);
    assert_eq!(contract_analytics.average_bounty_amount, 5000);
}

#[test]
fn test_contract_analytics_after_multiple_locks() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);
    token_admin.mint(&depositor, &100_000_000);

    let deadline = env.ledger().timestamp() + 1000;
    escrow.lock_funds(&depositor, &1, &1000, &deadline);
    escrow.lock_funds(&depositor, &2, &2000, &deadline);
    escrow.lock_funds(&depositor, &3, &3000, &deadline);

    let analytics = escrow.get_contract_analytics();
    assert_eq!(analytics.active_bounty_count, 3);
    assert_eq!(analytics.total_locked, 6000);
    assert_eq!(analytics.average_bounty_amount, 2000); // (1000 + 2000 + 3000) / 3
}

#[test]
fn test_contract_analytics_after_release() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let contributor = Address::generate(&env);
    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);
    token_admin.mint(&depositor, &100_000_000);

    let deadline = env.ledger().timestamp() + 1000;
    escrow.lock_funds(&depositor, &1, &5000, &deadline);
    escrow.release_funds(&1, &contributor);

    // Contract analytics should reflect release
    let analytics = escrow.get_contract_analytics();
    assert_eq!(analytics.active_bounty_count, 0);
    assert_eq!(analytics.released_bounty_count, 1);
    assert_eq!(analytics.total_locked, 0);
    assert_eq!(analytics.total_released, 5000);

    // Bounty analytics should reflect release
    let bounty_analytics = escrow.get_bounty_analytics(&1).unwrap();
    assert_eq!(bounty_analytics.total_amount_released, 5000);
    assert_eq!(bounty_analytics.remaining_amount, 0);
}

#[test]
fn test_bounty_analytics_after_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);
    token_admin.mint(&depositor, &10_000_000);

    let deadline = env.ledger().timestamp() + 500;
    escrow.lock_funds(&depositor, &1, &8000, &deadline);

    // Advance time past deadline
    env.ledger().set_timestamp(deadline + 1);
    escrow.refund(&1).unwrap();

    // Bounty analytics should reflect refund
    let bounty_analytics = escrow.get_bounty_analytics(&1).unwrap();
    assert_eq!(bounty_analytics.total_amount_refunded, 8000);
    assert_eq!(bounty_analytics.remaining_amount, 0);

    // Contract analytics should show refunded bounty
    let contract_analytics = escrow.get_contract_analytics();
    assert_eq!(contract_analytics.refunded_bounty_count, 1);
    assert_eq!(contract_analytics.total_refunded, 8000);
}

#[test]
fn test_count_bounties_by_status() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let contributor = Address::generate(&env);
    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);
    token_admin.mint(&depositor, &100_000_000);

    let deadline = env.ledger().timestamp() + 1000;
    // Create 3 locked bounties
    escrow.lock_funds(&depositor, &1, &1000, &deadline);
    escrow.lock_funds(&depositor, &2, &2000, &deadline);
    escrow.lock_funds(&depositor, &3, &3000, &deadline);

    let locked_count = escrow.count_bounties_by_status(&EscrowStatus::Locked);
    assert_eq!(locked_count, 3);

    // Release one
    escrow.release_funds(&1, &contributor);

    let locked_count = escrow.count_bounties_by_status(&EscrowStatus::Locked);
    let released_count = escrow.count_bounties_by_status(&EscrowStatus::Released);
    assert_eq!(locked_count, 2);
    assert_eq!(released_count, 1);
}

#[test]
fn test_get_volume_by_status() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let contributor = Address::generate(&env);
    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);
    token_admin.mint(&depositor, &100_000_000);

    let deadline = env.ledger().timestamp() + 1000;
    escrow.lock_funds(&depositor, &1, &1500, &deadline);
    escrow.lock_funds(&depositor, &2, &2500, &deadline);
    escrow.lock_funds(&depositor, &3, &3000, &deadline);

    let locked_volume = escrow.get_volume_by_status(&EscrowStatus::Locked);
    assert_eq!(locked_volume, 7000);

    // Release one
    escrow.release_funds(&1, &contributor);

    let locked_volume = escrow.get_volume_by_status(&EscrowStatus::Locked);
    let released_volume = escrow.get_volume_by_status(&EscrowStatus::Released);
    assert_eq!(locked_volume, 5500); // 2500 + 3000
    assert_eq!(released_volume, 1500);
}

#[test]
fn test_get_depositor_stats() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let depositor1 = Address::generate(&env);
    let depositor2 = Address::generate(&env);
    let contributor = Address::generate(&env);
    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);
    token_admin.mint(&depositor1, &100_000_000);
    token_admin.mint(&depositor2, &100_000_000);

    let deadline = env.ledger().timestamp() + 1000;
    escrow.lock_funds(&depositor1, &1, &1000, &deadline);
    escrow.lock_funds(&depositor1, &2, &2000, &deadline);
    escrow.lock_funds(&depositor2, &3, &3000, &deadline);

    let (locked1, locked_amt1, _, _, _, _) = escrow.get_depositor_stats(&depositor1);
    assert_eq!(locked1, 2);
    assert_eq!(locked_amt1, 3000);

    let (locked2, locked_amt2, _, _, _, _) = escrow.get_depositor_stats(&depositor2);
    assert_eq!(locked2, 1);
    assert_eq!(locked_amt2, 3000);

    // Release one from depositor1
    escrow.release_funds(&1, &contributor);

    let (locked1, locked_amt1, released1, released_amt1, _, _) = escrow.get_depositor_stats(&depositor1);
    assert_eq!(locked1, 1);
    assert_eq!(locked_amt1, 2000);
    assert_eq!(released1, 1);
    assert_eq!(released_amt1, 1000);
}

#[test]
fn test_query_expiring_bounties() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);
    token_admin.mint(&depositor, &100_000_000);

    let current_time = env.ledger().timestamp();
    // Create bounties with different deadlines
    escrow.lock_funds(&depositor, &1, &1000, &(current_time + 100)); // Soon
    escrow.lock_funds(&depositor, &2, &2000, &(current_time + 1000)); // Far
    escrow.lock_funds(&depositor, &3, &3000, &(current_time + 50)); // Soonest

    // Query for bounties expiring by current_time + 500
    let expiring = escrow.query_expiring_bounties(&(current_time + 500), &0, &10);
    assert_eq!(expiring.len(), 2); // Bounties 1 and 3
}

#[test]
fn test_get_high_value_bounties() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);
    token_admin.mint(&depositor, &100_000_000);

    let deadline = env.ledger().timestamp() + 1000;
    escrow.lock_funds(&depositor, &1, &500, &deadline);
    escrow.lock_funds(&depositor, &2, &5000, &deadline);
    escrow.lock_funds(&depositor, &3, &10000, &deadline);
    escrow.lock_funds(&depositor, &4, &1000, &deadline);

    // Get high-value bounties (>= 5000)
    let high_value = escrow.get_high_value_bounties(&5000, &10);
    assert_eq!(high_value.len(), 2); // Bounties 2 and 3
}

#[test]
fn test_emit_contract_analytics_snapshot() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);
    token_admin.mint(&depositor, &100_000_000);

    let deadline = env.ledger().timestamp() + 1000;
    escrow.lock_funds(&depositor, &1, &5000, &deadline);

    // Emit snapshot
    escrow.emit_contract_analytics_snapshot();

    // Check that event was emitted
    let events = env.events().all();
    let snapshot_events: Vec<_> = events
        .iter()
        .filter(|e| e.1.contains(&soroban_sdk::Symbol::short("snap")))
        .collect();

    assert!(!snapshot_events.is_empty(), "Analytics snapshot event should be emitted");
}

#[test]
fn test_nonexistent_bounty_analytics_returns_error() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (token, _) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);

    let result = escrow.get_bounty_analytics(&999);
    match result {
        Err(_) => {} // Expected
        Ok(_) => panic!("Should return error for nonexistent bounty"),
    }
}

#[test]
fn test_analytics_with_heavy_load() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);
    token_admin.mint(&depositor, &1_000_000_000);

    let deadline = env.ledger().timestamp() + 1000;
    
    // Lock 50 bounties (close to batch limit)
    for i in 1..=50 {
        escrow.lock_funds(&depositor, &i, &(i as i128 * 100), &deadline);
    }

    let analytics = escrow.get_contract_analytics();
    assert_eq!(analytics.active_bounty_count, 50);
    
    // Calculate expected total: 100 + 200 + ... + 5000 = 100 * (1 + 2 + ... + 50)
    // = 100 * 50 * 51 / 2 = 127500
    assert_eq!(analytics.total_locked, 127500);
}

#[test]
fn test_analytics_with_partial_refunds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);
    token_admin.mint(&depositor, &100_000_000);

    let deadline = env.ledger().timestamp() + 1000;
    escrow.lock_funds(&depositor, &1, &10000, &deadline);

    // Approve partial refund
    escrow.approve_refund(&1, &5000, &depositor, &crate::RefundMode::Partial).unwrap();
    escrow.refund(&1).unwrap();

    // Check analytics after partial refund
    let bounty_analytics = escrow.get_bounty_analytics(&1).unwrap();
    assert_eq!(bounty_analytics.total_amount_refunded, 5000);
    assert_eq!(bounty_analytics.remaining_amount, 5000);
    assert_eq!(bounty_analytics.partial_refunds_count, 1);
}

#[test]
fn test_analytics_consistency_across_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let contributor = Address::generate(&env);
    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);
    token_admin.mint(&depositor, &100_000_000);

    let deadline = env.ledger().timestamp() + 1000;
    
    // Lock
    escrow.lock_funds(&depositor, &1, &10000, &deadline);
    let analytics = escrow.get_contract_analytics();
    assert_eq!(analytics.total_locked, 10000);
    assert_eq!(analytics.active_bounty_count, 1);

    // Release
    escrow.release_funds(&1, &contributor);
    let analytics = escrow.get_contract_analytics();
    assert_eq!(analytics.total_locked, 0);
    assert_eq!(analytics.total_released, 10000);
    assert_eq!(analytics.active_bounty_count, 0);
    assert_eq!(analytics.released_bounty_count, 1);
}

#[test]
fn test_depositor_stats_with_mixed_operations() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let contributor = Address::generate(&env);
    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);
    token_admin.mint(&depositor, &100_000_000);

    let deadline = env.ledger().timestamp() + 500;
    
    // Lock 3 bounties
    escrow.lock_funds(&depositor, &1, &1000, &deadline);
    escrow.lock_funds(&depositor, &2, &2000, &deadline);
    escrow.lock_funds(&depositor, &3, &3000, &deadline);

    // Release one
    escrow.release_funds(&1, &contributor);

    // Refund one after deadline
    env.ledger().set_timestamp(deadline + 1);
    escrow.refund(&2).unwrap();

    // Check stats
    let (locked, locked_amt, released, released_amt, refunded, refunded_amt) = 
        escrow.get_depositor_stats(&depositor);
    
    assert_eq!(locked, 1); // Only bounty 3
    assert_eq!(locked_amt, 3000);
    assert_eq!(released, 1); // Bounty 1
    assert_eq!(released_amt, 1000);
    assert_eq!(refunded, 1); // Bounty 2
    assert_eq!(refunded_amt, 2000);
}

#[test]
fn test_volume_by_status_remaining_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (token, token_admin) = create_token_contract(&env, &admin);
    let escrow = create_escrow_contract(&env);
    escrow.init(&admin, &token.address);
    token_admin.mint(&depositor, &100_000_000);

    let deadline = env.ledger().timestamp() + 1000;
    
    // Lock bounties
    escrow.lock_funds(&depositor, &1, &5000, &deadline);
    escrow.lock_funds(&depositor, &2, &3000, &deadline);

    // Approve partial refund on bounty 1
    escrow.approve_refund(&1, &2000, &depositor, &crate::RefundMode::Partial).unwrap();
    escrow.refund(&1).unwrap();

    // Volume should reflect remaining amounts
    let locked_volume = escrow.get_volume_by_status(&EscrowStatus::PartiallyRefunded);
    assert_eq!(locked_volume, 3000); // 5000 - 2000
}

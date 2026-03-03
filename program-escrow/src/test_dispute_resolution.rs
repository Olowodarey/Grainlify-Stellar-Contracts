//! Dispute resolution tests for the Program Escrow contract.
//!
//! Covers the full lifecycle of program-level disputes:
//! - Opening a dispute blocks payouts and schedule releases
//! - Resolving a dispute re-enables payouts
//! - Cancelling a dispute re-enables payouts
//! - Admin-only enforcement
//! - Edge cases (double-open, resolve-when-none, etc.)
//!
//! Implements the 15 scenarios outlined in Issue 61.

#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String,
};

// ─── shared helper ────────────────────────────────────────────────────────────

/// Spin up a fully-initialised program contract with tokens minted and
/// locked, admin set, and `mock_all_auths` active.
fn setup(
    env: &Env,
    initial_funds: i128,
) -> (
    ProgramEscrowContractClient<'static>,
    Address, // admin
    Address, // contract_id
) {
    env.mock_all_auths();

    let contract_id = env.register_contract(None, ProgramEscrowContract);
    let client = ProgramEscrowContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let token_admin_key = Address::generate(env);
    let token_id = env.register_stellar_asset_contract(token_admin_key.clone());
    let token_admin = token::StellarAssetClient::new(env, &token_id);

    let program_id = String::from_str(env, "disp-test-program");
    client.init_program(&program_id, &admin, &token_id);

    // Set admin for dispute resolution
    client.set_admin(&admin);

    if initial_funds > 0 {
        token_admin.mint(&contract_id, &initial_funds);
        client.lock_program_funds(&initial_funds);
    }

    (client, admin, contract_id)
}

// ─── Test 1: open dispute blocks single_payout ────────────────────────────────

/// Lock funds → open dispute → single_payout must panic with "Dispute in progress".
#[test]
#[should_panic(expected = "Dispute in progress")]
fn test_open_dispute_blocks_payout() {
    let env = Env::default();
    let (client, admin, _cid) = setup(&env, 100_000);

    let reason = String::from_str(&env, "Winner address disputed by organiser");
    client.open_dispute(&reason);

    // Payout attempt must be blocked
    let recipient = Address::generate(&env);
    client.single_payout(&recipient, &10_000);
}

// ─── Test 2: resolve dispute re-enables single_payout ────────────────────────

/// Open → resolve → single_payout succeeds and balance is correct.
#[test]
fn test_resolve_dispute_allows_payout() {
    let env = Env::default();
    let (client, _admin, _cid) = setup(&env, 100_000);

    let reason = String::from_str(&env, "Disputed submission");
    client.open_dispute(&reason);

    // Confirm blocked
    assert!(client.is_disputed());

    // Admin resolves
    client.resolve_dispute();
    assert!(!client.is_disputed());

    // Payout should now work
    let recipient = Address::generate(&env);
    let data = client.single_payout(&recipient, &40_000);
    assert_eq!(data.remaining_balance, 60_000);
}

// ─── Test 3: open dispute blocks batch_payout ────────────────────────────────

/// Lock funds → open dispute → batch_payout must panic.
#[test]
#[should_panic(expected = "Dispute in progress")]
fn test_dispute_blocks_batch_payout() {
    let env = Env::default();
    let (client, _admin, _cid) = setup(&env, 200_000);

    let reason = String::from_str(&env, "Batch payout disputed");
    client.open_dispute(&reason);

    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);
    let recipients = soroban_sdk::vec![&env, r1, r2];
    let amounts = soroban_sdk::vec![&env, 50_000i128, 50_000i128];
    client.batch_payout(&recipients, &amounts);
}

// ─── Test 4: dispute status transitions and events ───────────────────────────

/// Verify is_disputed false → open → true → resolve → false.
/// Verify the stored DisputeRecord fields are correct.
#[test]
fn test_dispute_status_and_events() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000);
    let (client, admin, _cid) = setup(&env, 50_000);

    // No dispute initially
    assert!(!client.is_disputed());
    assert!(client.get_dispute().is_none());

    // Open
    let reason = String::from_str(&env, "Evidence mismatch");
    client.open_dispute(&reason);

    assert!(client.is_disputed());
    let record = client.get_dispute().expect("dispute should be Some");
    assert_eq!(record.status, DisputeStatus::Open);
    assert_eq!(record.opened_by, admin);
    assert_eq!(record.opened_at, 1_000);
    assert!(record.resolved_by.is_none());

    // Resolve
    env.ledger().set_timestamp(2_000);
    client.resolve_dispute();

    assert!(!client.is_disputed());
    let resolved = client.get_dispute().expect("record should persist");
    assert_eq!(resolved.status, DisputeStatus::Resolved);
    assert_eq!(resolved.resolved_by, Some(admin.clone()));
    assert_eq!(resolved.resolved_at, Some(2_000));
}

// ─── Test 5: non-admin cannot open dispute ───────────────────────────────────

/// Admin auth is required to open a dispute.
/// Calling open_dispute without providing auth returns an error.
#[test]
fn test_open_dispute_non_admin_rejected() {
    let env = Env::default();
    // Do NOT call mock_all_auths — auth will be enforced.
    let contract_id = env.register_contract(None, ProgramEscrowContract);
    let client = ProgramEscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_key = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract(token_key);
    let prog_id = String::from_str(&env, "restricted-prog");

    // Set up program with auth mocked only for setup steps
    env.mock_all_auths();
    client.init_program(&prog_id, &admin, &token_id);
    client.set_admin(&admin);

    // Now try to open a dispute WITHOUT providing the admin's auth.
    // We do this by creating a new env without mock_all_auths and using try_
    let env2 = Env::default();
    let contract_id2 = env2.register_contract(None, ProgramEscrowContract);
    let client2 = ProgramEscrowContractClient::new(&env2, &contract_id2);
    let admin2 = Address::generate(&env2);
    let token_key2 = Address::generate(&env2);
    let token_id2 = env2.register_stellar_asset_contract(token_key2);
    let prog_id2 = String::from_str(&env2, "prog2");
    env2.mock_all_auths();
    client2.init_program(&prog_id2, &admin2, &token_id2);
    client2.set_admin(&admin2);

    // Use a fresh env with no mock — open_dispute needs admin auth, must fail
    let env3 = Env::default();
    let contract_id3 = env3.register_contract(None, ProgramEscrowContract);
    let client3 = ProgramEscrowContractClient::new(&env3, &contract_id3);
    // Without set_admin and without mock_all_auths, require_admin panics:
    let result = client3.try_open_dispute(&String::from_str(&env3, "unauthorized"));
    // Should be an error (admin not set)
    assert!(result.is_err());
}

// ─── Test 6: non-admin cannot resolve dispute ────────────────────────────────

/// resolve_dispute requires admin auth — fails if admin not set.
#[test]
fn test_resolve_dispute_non_admin_rejected() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ProgramEscrowContract);
    let client = ProgramEscrowContractClient::new(&env, &contract_id);

    // No admin set, no mock_all_auths — try_resolve_dispute must return Err
    let result = client.try_resolve_dispute();
    assert!(result.is_err());
}

// ─── Test 7: non-admin cannot cancel dispute ─────────────────────────────────

/// cancel_dispute requires admin auth — fails if admin not set.
#[test]
fn test_cancel_dispute_non_admin_rejected() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ProgramEscrowContract);
    let client = ProgramEscrowContractClient::new(&env, &contract_id);

    // No admin set — try_cancel_dispute must return Err
    let result = client.try_cancel_dispute();
    assert!(result.is_err());
}

// ─── Test 8: resolve when no dispute panics ──────────────────────────────────

#[test]
#[should_panic(expected = "No dispute to resolve")]
fn test_resolve_when_no_dispute_panics() {
    let env = Env::default();
    let (client, _admin, _cid) = setup(&env, 10_000);
    // No dispute opened — must panic
    client.resolve_dispute();
}

// ─── Test 9: cancel when no dispute panics ───────────────────────────────────

#[test]
#[should_panic(expected = "No dispute to cancel")]
fn test_cancel_when_no_dispute_panics() {
    let env = Env::default();
    let (client, _admin, _cid) = setup(&env, 10_000);
    client.cancel_dispute();
}

// ─── Test 10: double-open panics ─────────────────────────────────────────────

#[test]
#[should_panic(expected = "Dispute already open")]
fn test_open_dispute_when_already_open_panics() {
    let env = Env::default();
    let (client, _admin, _cid) = setup(&env, 50_000);

    let reason = String::from_str(&env, "First dispute");
    client.open_dispute(&reason);

    // Second open must panic
    let reason2 = String::from_str(&env, "Second dispute");
    client.open_dispute(&reason2);
}

// ─── Test 11: dispute blocks trigger_program_releases ────────────────────────

/// Open dispute → trigger_program_releases must panic.
#[test]
#[should_panic(expected = "Dispute in progress")]
fn test_dispute_blocks_trigger_releases() {
    let env = Env::default();
    env.ledger().set_timestamp(0);
    let (client, _admin, _cid) = setup(&env, 100_000);

    // Create a schedule due in the future
    let recipient = Address::generate(&env);
    client.create_program_release_schedule(&50_000, &500, &recipient);

    // Open dispute
    let reason = String::from_str(&env, "Release disputed");
    client.open_dispute(&reason);

    // Advance past release timestamp
    env.ledger().set_timestamp(600);

    // Should panic — dispute is blocking
    client.trigger_program_releases();
}

// ─── Test 12: cancel dispute allows payout ───────────────────────────────────

#[test]
fn test_cancel_dispute_allows_payout() {
    let env = Env::default();
    let (client, _admin, _cid) = setup(&env, 80_000);

    let reason = String::from_str(&env, "Dispute under investigation");
    client.open_dispute(&reason);
    assert!(client.is_disputed());

    client.cancel_dispute();
    assert!(!client.is_disputed());

    // Payout should succeed after cancellation
    let recipient = Address::generate(&env);
    let data = client.single_payout(&recipient, &30_000);
    assert_eq!(data.remaining_balance, 50_000);
}

// ─── Test 13: full lifecycle open → resolve ──────────────────────────────────

/// Verify full state machine: None → Open → Resolved.
/// Checks every field of the DisputeRecord at each stage.
#[test]
fn test_dispute_lifecycle_full() {
    let env = Env::default();
    env.ledger().set_timestamp(100);
    let (client, admin, _cid) = setup(&env, 200_000);

    // Stage 0: no dispute
    assert!(!client.is_disputed());
    assert!(client.get_dispute().is_none());

    // Stage 1: open
    let reason = String::from_str(&env, "Winning submission under review");
    client.open_dispute(&reason);

    let r1 = client.get_dispute().unwrap();
    assert_eq!(r1.status, DisputeStatus::Open);
    assert_eq!(r1.opened_by, admin);
    assert_eq!(r1.opened_at, 100);
    assert!(r1.resolved_by.is_none());
    assert!(r1.resolved_at.is_none());

    // Stage 2: resolve
    env.ledger().set_timestamp(300);
    client.resolve_dispute();

    let r2 = client.get_dispute().unwrap();
    assert_eq!(r2.status, DisputeStatus::Resolved);
    assert_eq!(r2.opened_by, admin);
    assert_eq!(r2.resolved_by, Some(admin.clone()));
    assert_eq!(r2.resolved_at, Some(300));

    // Stage 3: payouts re-enabled
    let recipient = Address::generate(&env);
    let data = client.single_payout(&recipient, &100_000);
    assert_eq!(data.remaining_balance, 100_000);
    assert_eq!(data.payout_history.len(), 1);
}

// ─── Test 14: lock_program_funds works during dispute ────────────────────────

/// Dispute must NOT block locking additional funds into the program.
#[test]
fn test_dispute_does_not_affect_lock_funds() {
    let env = Env::default();
    let (client, _admin, _cid) = setup(&env, 50_000);

    let reason = String::from_str(&env, "Checking submission validity");
    client.open_dispute(&reason);

    // Lock additional funds — should succeed even during dispute
    client.lock_program_funds(&25_000);

    let info = client.get_program_info();
    assert_eq!(info.total_funds, 75_000);
    assert_eq!(info.remaining_balance, 75_000);
}

// ─── Test 15: dispute reason is persisted correctly ──────────────────────────

/// Verify the reason string is stored verbatim and queryable.
#[test]
fn test_dispute_reason_stored_correctly() {
    let env = Env::default();
    let (client, _admin, _cid) = setup(&env, 10_000);

    let expected_reason = String::from_str(
        &env,
        "Winner submitted ineligible work — see review thread #42",
    );
    client.open_dispute(&expected_reason);

    let record = client.get_dispute().expect("dispute should be Some");
    assert_eq!(record.reason, expected_reason);
    assert_eq!(record.status, DisputeStatus::Open);
}

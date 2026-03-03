#![cfg(test)]

use crate::{BountyEscrowContract, BountyEscrowContractClient, EscrowStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

fn create_token_contract<'a>(
    e: &Env,
    admin: &Address,
) -> (token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract_address = e.register_stellar_asset_contract(admin.clone());
    (
        token::Client::new(e, &contract_address),
        token::StellarAssetClient::new(e, &contract_address),
    )
}

fn create_escrow_contract<'a>(e: &Env) -> BountyEscrowContractClient<'a> {
    let contract_id = e.register_contract(None, BountyEscrowContract);
    BountyEscrowContractClient::new(e, &contract_id)
}

struct DisputeTestSetup<'a> {
    env: Env,
    admin: Address,
    depositor: Address,
    contributor: Address,
    token: token::Client<'a>,
    token_admin: token::StellarAssetClient<'a>,
    escrow: BountyEscrowContractClient<'a>,
}

impl<'a> DisputeTestSetup<'a> {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let depositor = Address::generate(&env);
        let contributor = Address::generate(&env);

        let (token, token_admin) = create_token_contract(&env, &admin);
        let escrow = create_escrow_contract(&env);

        escrow.init(&admin, &token.address);

        // Mint tokens to depositor
        token_admin.mint(&depositor, &10_000_000);

        Self {
            env,
            admin,
            depositor,
            contributor,
            token,
            token_admin,
            escrow,
        }
    }
}

/// Opening a dispute via `authorize_claim` must block direct release.
#[test]
fn test_open_dispute_blocks_release() {
    let setup = DisputeTestSetup::new();
    let bounty_id = 100;
    let amount = 1_000;
    let now = setup.env.ledger().timestamp();
    let deadline = now + 1_000;
    let claim_window = 600;

    setup.escrow.set_claim_window(&claim_window);
    setup
        .escrow
        .lock_funds(&setup.depositor, &bounty_id, &amount, &deadline);

    // Open dispute by authorizing a claim for the contributor
    setup.escrow.authorize_claim(&bounty_id, &setup.contributor);

    // Attempt direct release while dispute is open
    let res = setup
        .escrow
        .try_release_funds(&bounty_id, &setup.contributor);
    assert!(res.is_err(), "release_funds must be blocked by dispute");

    // State remains locked and funds remain in escrow
    let escrow = setup.escrow.get_escrow_info(&bounty_id);
    assert_eq!(escrow.status, EscrowStatus::Locked);
    assert_eq!(setup.token.balance(&setup.escrow.address), amount);
}

/// Opening a dispute must block refunds, even after deadline passes, until resolved.
#[test]
fn test_open_dispute_blocks_refund() {
    let setup = DisputeTestSetup::new();
    let bounty_id = 101;
    let amount = 2_000;
    let now = setup.env.ledger().timestamp();
    let deadline = now + 1_000;
    let claim_window = 600;

    setup.escrow.set_claim_window(&claim_window);
    setup
        .escrow
        .lock_funds(&setup.depositor, &bounty_id, &amount, &deadline);

    // Open dispute
    setup.escrow.authorize_claim(&bounty_id, &setup.contributor);

    // Move past deadline
    setup.env.ledger().set_timestamp(deadline + 1);

    // Refund should be blocked while pending claim exists
    let res = setup.escrow.try_refund(&bounty_id);
    assert!(res.is_err(), "refund must be blocked by dispute");

    let escrow = setup.escrow.get_escrow_info(&bounty_id);
    assert_eq!(escrow.status, EscrowStatus::Locked);
    assert_eq!(setup.token.balance(&setup.escrow.address), amount);
}

/// Resolving dispute in favor of release: beneficiary claims within window.
#[test]
fn test_resolve_dispute_in_favor_of_release() {
    let setup = DisputeTestSetup::new();
    let bounty_id = 102;
    let amount = 3_000;
    let now = setup.env.ledger().timestamp();
    let deadline = now + 2_000;
    let claim_window = 500;

    setup.escrow.set_claim_window(&claim_window);
    setup
        .escrow
        .lock_funds(&setup.depositor, &bounty_id, &amount, &deadline);

    setup.escrow.authorize_claim(&bounty_id, &setup.contributor);
    let claim = setup.escrow.get_pending_claim(&bounty_id);

    // Beneficiary claims within the dispute window
    setup.env.ledger().set_timestamp(claim.expires_at - 1);
    setup.escrow.claim(&bounty_id);

    let escrow = setup.escrow.get_escrow_info(&bounty_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(setup.token.balance(&setup.contributor), amount);
    assert_eq!(setup.token.balance(&setup.escrow.address), 0);
}

/// Resolving dispute in favor of refund: admin cancels then refunds after deadline.
#[test]
fn test_resolve_dispute_in_favor_of_refund() {
    let setup = DisputeTestSetup::new();
    let bounty_id = 103;
    let amount = 4_000;
    let now = setup.env.ledger().timestamp();
    let deadline = now + 2_000;
    let claim_window = 500;

    setup.escrow.set_claim_window(&claim_window);
    setup
        .escrow
        .lock_funds(&setup.depositor, &bounty_id, &amount, &deadline);

    // Open dispute
    setup.escrow.authorize_claim(&bounty_id, &setup.contributor);

    // Let claim window expire, but before refund can happen, admin must cancel
    let claim = setup.escrow.get_pending_claim(&bounty_id);
    setup.env.ledger().set_timestamp(claim.expires_at + 1);

    // Cancel pending claim to resolve dispute
    setup.escrow.cancel_pending_claim(&bounty_id);

    // Move past original deadline, then refund succeeds
    setup.env.ledger().set_timestamp(deadline + 1);
    setup.escrow.refund(&bounty_id);

    let escrow = setup.escrow.get_escrow_info(&bounty_id);
    assert_eq!(escrow.status, EscrowStatus::Refunded);
    assert_eq!(setup.token.balance(&setup.depositor), 10_000_000);
    assert_eq!(setup.token.balance(&setup.escrow.address), 0);
}

/// Dispute status is implicitly represented by the presence of a PendingClaim.
#[test]
fn test_dispute_status_tracking() {
    let setup = DisputeTestSetup::new();
    let bounty_id = 104;
    let amount = 1_500;
    let now = setup.env.ledger().timestamp();
    let deadline = now + 1_000;
    let claim_window = 400;

    setup.escrow.set_claim_window(&claim_window);
    setup
        .escrow
        .lock_funds(&setup.depositor, &bounty_id, &amount, &deadline);

    // Initially, no pending claim for this bounty
    let pending_before = setup.escrow.try_get_pending_claim(&bounty_id);
    assert!(
        pending_before.is_err(),
        "no dispute should be open initially"
    );

    // Open dispute
    setup.escrow.authorize_claim(&bounty_id, &setup.contributor);
    let claim = setup.escrow.get_pending_claim(&bounty_id);
    assert_eq!(claim.bounty_id, bounty_id);
    assert_eq!(claim.recipient, setup.contributor);
    assert!(!claim.claimed);

    // Resolve dispute by cancelling claim, then verify it's cleared
    setup.escrow.cancel_pending_claim(&bounty_id);
    let pending_after = setup.escrow.try_get_pending_claim(&bounty_id);
    assert!(
        pending_after.is_err(),
        "dispute should be cleared after cancellation"
    );
}

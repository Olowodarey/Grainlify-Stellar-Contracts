//! Standalone reentrancy guard tests that can be compiled independently
//!
//! These tests verify the core reentrancy guard functionality without
//! depending on the full contract implementation.

#![cfg(test)]

use crate::{reentrancy_guard::*, ProgramEscrowContract};
use soroban_sdk::Env;

/// Helper to execute a closure within a contract context.
fn with_contract_env<F, T>(f: F) -> T
where
    F: FnOnce(Env) -> T,
{
    let env = Env::default();
    let contract_id = env.register_contract(None, ProgramEscrowContract);
    env.as_contract(&contract_id, || f(env.clone()))
}

#[test]
fn test_guard_initially_not_set() {
    with_contract_env(|env| {
        assert!(!is_entered(&env), "Guard should not be set initially");
    });
}

#[test]
fn test_guard_can_be_set_and_cleared() {
    with_contract_env(|env| {
        // Initially not set
        assert!(!is_entered(&env));

        // Set the guard
        set_entered(&env);
        assert!(is_entered(&env), "Guard should be set after set_entered");

        // Clear the guard
        clear_entered(&env);
        assert!(
            !is_entered(&env),
            "Guard should be cleared after clear_entered"
        );
    });
}

#[test]
fn test_check_passes_when_not_entered() {
    with_contract_env(|env| {
        // Should not panic
        check_not_entered(&env);
    });
}

#[test]
#[should_panic(expected = "Reentrancy detected")]
fn test_check_panics_when_entered() {
    with_contract_env(|env| {
        // Set the guard
        set_entered(&env);

        // This should panic
        check_not_entered(&env);
    });
}

#[test]
fn test_multiple_set_clear_cycles() {
    with_contract_env(|env| {
        for _ in 0..5 {
            // Check passes
            check_not_entered(&env);

            // Set guard
            set_entered(&env);
            assert!(is_entered(&env));

            // Clear guard
            clear_entered(&env);
            assert!(!is_entered(&env));
        }
    });
}

#[test]
fn test_guard_state_persistence() {
    with_contract_env(|env| {
        // Set guard
        set_entered(&env);

        // Verify it persists across multiple checks
        assert!(is_entered(&env));
        assert!(is_entered(&env));
        assert!(is_entered(&env));

        // Clear and verify
        clear_entered(&env);
        assert!(!is_entered(&env));
        assert!(!is_entered(&env));
    });
}

#[test]
#[should_panic(expected = "Reentrancy detected")]
fn test_double_set_detected() {
    with_contract_env(|env| {
        // First set
        set_entered(&env);

        // Check should fail
        check_not_entered(&env);
    });
}

#[test]
fn test_clear_when_not_set_is_safe() {
    with_contract_env(|env| {
        // Clearing when not set should be safe
        clear_entered(&env);
        assert!(!is_entered(&env));

        // Can still set after clearing
        set_entered(&env);
        assert!(is_entered(&env));
    });
}

#[test]
fn test_guard_isolation_between_envs() {
    let env = Env::default();
    let contract_id_1 = env.register_contract(None, ProgramEscrowContract);
    let contract_id_2 = env.register_contract(None, ProgramEscrowContract);

    // Set guard in contract 1
    env.as_contract(&contract_id_1, || {
        set_entered(&env);
        assert!(is_entered(&env));
    });

    // Contract 2 should not be affected
    env.as_contract(&contract_id_2, || {
        assert!(!is_entered(&env));
        set_entered(&env);
        assert!(is_entered(&env));
    });

    // Both should be set
    env.as_contract(&contract_id_1, || {
        assert!(is_entered(&env));
        clear_entered(&env);
        assert!(!is_entered(&env));
    });

    // Contract 2 should still be set
    env.as_contract(&contract_id_2, || {
        assert!(is_entered(&env));
    });
}

#[test]
fn test_sequential_protected_operations() {
    with_contract_env(|env| {
        // Simulate 3 sequential protected operations
        for i in 0..3 {
            // Check guard is clear
            check_not_entered(&env);

            // Set guard (operation starts)
            set_entered(&env);

            // Verify guard is set
            assert!(
                is_entered(&env),
                "Guard should be set during operation {}",
                i
            );

            // Clear guard (operation completes)
            clear_entered(&env);

            // Verify guard is cleared
            assert!(
                !is_entered(&env),
                "Guard should be cleared after operation {}",
                i
            );
        }
    });
}

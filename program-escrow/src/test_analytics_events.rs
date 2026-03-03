#![cfg(test)]

use crate::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, vec, Address, Env, Map, String as SorobanString, Symbol, TryFromVal, Val,
};

fn setup_program(
    env: &Env,
    initial_amount: i128,
) -> (
    ProgramEscrowContractClient<'static>,
    Address,
    token::Client<'static>,
    token::StellarAssetClient<'static>,
) {
    env.mock_all_auths();

    let contract_id = env.register_contract(None, ProgramEscrowContract);
    let client = ProgramEscrowContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::Client::new(env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(env, &token_id);

    let program_id = SorobanString::from_str(env, "hack-2026");
    client.init_program(&program_id, &admin, &token_id);

    if initial_amount > 0 {
        token_admin_client.mint(&client.address, &initial_amount);
        client.lock_program_funds(&initial_amount);
    }

    (client, admin, token_client, token_admin_client)
}

fn find_event_by_topic(env: &Env, topic: Symbol) -> Option<(Vec<Val>, Val)> {
    let events = env.events().all();
    for i in 0..events.len() {
        let event = events.get(i).unwrap();
        let topics = event.0;
        if topics.len() > 0 {
            let first_topic = topics.get(0).unwrap();
            if let Ok(sym) = Symbol::try_from_val(env, &first_topic) {
                if sym == topic {
                    return Some((topics, event.1));
                }
            }
        }
    }
    None
}

fn assert_event_has_version(env: &Env, data: &Val) {
    let data_map: Map<Symbol, Val> =
        Map::try_from_val(env, data).unwrap_or_else(|_| panic!("event payload should be a map"));
    let version_val = data_map
        .get(Symbol::new(env, "version"))
        .unwrap_or_else(|| panic!("event payload must contain version field"));
    let version = u32::try_from_val(env, &version_val).expect("version should decode as u32");
    assert_eq!(version, 2);
}

#[test]
fn test_aggregate_stats_event_on_single_payout() {
    let env = Env::default();
    let (client, _admin, _token, _token_admin) = setup_program(&env, 100_000);

    let recipient = Address::generate(&env);
    client.single_payout(&recipient, &30_000);

    // Find aggregate stats event
    let event = find_event_by_topic(&env, symbol_short!("AggStats"));
    assert!(event.is_some(), "AggregateStats event should be emitted");

    let (_, data) = event.unwrap();
    assert_event_has_version(&env, &data);

    // Verify event structure
    let data_map: Map<Symbol, Val> = Map::try_from_val(&env, &data).unwrap();
    
    let total_funds = i128::try_from_val(&env, &data_map.get(Symbol::new(&env, "total_funds")).unwrap()).unwrap();
    let remaining_balance = i128::try_from_val(&env, &data_map.get(Symbol::new(&env, "remaining_balance")).unwrap()).unwrap();
    let total_paid_out = i128::try_from_val(&env, &data_map.get(Symbol::new(&env, "total_paid_out")).unwrap()).unwrap();
    let payout_count = u32::try_from_val(&env, &data_map.get(Symbol::new(&env, "payout_count")).unwrap()).unwrap();

    assert_eq!(total_funds, 100_000);
    assert_eq!(remaining_balance, 70_000);
    assert_eq!(total_paid_out, 30_000);
    assert_eq!(payout_count, 1);
}

#[test]
fn test_aggregate_stats_event_on_batch_payout() {
    let env = Env::default();
    let (client, _admin, _token, _token_admin) = setup_program(&env, 150_000);

    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);
    let r3 = Address::generate(&env);

    let recipients = vec![&env, r1.clone(), r2.clone(), r3.clone()];
    let amounts = vec![&env, 10_000, 20_000, 30_000];

    client.batch_payout(&recipients, &amounts);

    // Find aggregate stats event
    let event = find_event_by_topic(&env, symbol_short!("AggStats"));
    assert!(event.is_some(), "AggregateStats event should be emitted");

    let (_, data) = event.unwrap();
    assert_event_has_version(&env, &data);

    let data_map: Map<Symbol, Val> = Map::try_from_val(&env, &data).unwrap();
    
    let total_funds = i128::try_from_val(&env, &data_map.get(Symbol::new(&env, "total_funds")).unwrap()).unwrap();
    let remaining_balance = i128::try_from_val(&env, &data_map.get(Symbol::new(&env, "remaining_balance")).unwrap()).unwrap();
    let total_paid_out = i128::try_from_val(&env, &data_map.get(Symbol::new(&env, "total_paid_out")).unwrap()).unwrap();
    let payout_count = u32::try_from_val(&env, &data_map.get(Symbol::new(&env, "payout_count")).unwrap()).unwrap();

    assert_eq!(total_funds, 150_000);
    assert_eq!(remaining_balance, 90_000);
    assert_eq!(total_paid_out, 60_000);
    assert_eq!(payout_count, 3);
}

#[test]
fn test_large_payout_event_emitted_above_threshold() {
    let env = Env::default();
    let (client, _admin, _token, _token_admin) = setup_program(&env, 100_000);

    let recipient = Address::generate(&env);
    // Payout 15% of total funds (above 10% threshold)
    client.single_payout(&recipient, &15_000);

    // Find large payout event
    let event = find_event_by_topic(&env, symbol_short!("LrgPay"));
    assert!(event.is_some(), "LargePayout event should be emitted for payout above threshold");

    let (_, data) = event.unwrap();
    assert_event_has_version(&env, &data);

    let data_map: Map<Symbol, Val> = Map::try_from_val(&env, &data).unwrap();
    
    let amount = i128::try_from_val(&env, &data_map.get(Symbol::new(&env, "amount")).unwrap()).unwrap();
    let threshold = i128::try_from_val(&env, &data_map.get(Symbol::new(&env, "threshold")).unwrap()).unwrap();

    assert_eq!(amount, 15_000);
    assert_eq!(threshold, 10_000); // 10% of 100_000
}

#[test]
fn test_large_payout_event_not_emitted_below_threshold() {
    let env = Env::default();
    let (client, _admin, _token, _token_admin) = setup_program(&env, 100_000);

    let recipient = Address::generate(&env);
    // Payout 5% of total funds (below 10% threshold)
    client.single_payout(&recipient, &5_000);

    // Find large payout event
    let event = find_event_by_topic(&env, symbol_short!("LrgPay"));
    assert!(event.is_none(), "LargePayout event should NOT be emitted for payout below threshold");
}

#[test]
fn test_large_payout_event_in_batch() {
    let env = Env::default();
    let (client, _admin, _token, _token_admin) = setup_program(&env, 100_000);

    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);
    let r3 = Address::generate(&env);

    let recipients = vec![&env, r1.clone(), r2.clone(), r3.clone()];
    // One payout is 15% (above threshold), others are below
    let amounts = vec![&env, 5_000, 15_000, 3_000];

    client.batch_payout(&recipients, &amounts);

    // Count large payout events
    let events = env.events().all();
    let mut large_payout_count = 0;
    for i in 0..events.len() {
        let event = events.get(i).unwrap();
        let topics = event.0;
        if topics.len() > 0 {
            let first_topic = topics.get(0).unwrap();
            if let Ok(sym) = Symbol::try_from_val(&env, &first_topic) {
                if sym == symbol_short!("LrgPay") {
                    large_payout_count += 1;
                }
            }
        }
    }

    assert_eq!(large_payout_count, 1, "Exactly one LargePayout event should be emitted");
}

#[test]
fn test_schedule_triggered_event_automatic() {
    let env = Env::default();
    let (client, _admin, _token, _token_admin) = setup_program(&env, 100_000);

    let recipient = Address::generate(&env);
    let release_timestamp = 1000;

    // Create schedule
    client.create_program_release_schedule(&50_000, &release_timestamp, &recipient);

    // Advance time
    env.ledger().set_timestamp(release_timestamp + 1);

    // Trigger automatic release
    client.trigger_program_releases();

    // Find schedule triggered event
    let event = find_event_by_topic(&env, symbol_short!("SchedTrg"));
    assert!(event.is_some(), "ScheduleTriggered event should be emitted");

    let (_, data) = event.unwrap();
    assert_event_has_version(&env, &data);

    let data_map: Map<Symbol, Val> = Map::try_from_val(&env, &data).unwrap();
    
    let schedule_id = u64::try_from_val(&env, &data_map.get(Symbol::new(&env, "schedule_id")).unwrap()).unwrap();
    let amount = i128::try_from_val(&env, &data_map.get(Symbol::new(&env, "amount")).unwrap()).unwrap();

    assert_eq!(schedule_id, 1);
    assert_eq!(amount, 50_000);
}

#[test]
fn test_schedule_triggered_event_manual() {
    let env = Env::default();
    let (client, _admin, _token, _token_admin) = setup_program(&env, 100_000);

    let recipient = Address::generate(&env);
    let release_timestamp = 1000;

    // Create schedule
    client.create_program_release_schedule(&50_000, &release_timestamp, &recipient);

    // Manually release before timestamp
    env.ledger().set_timestamp(500);
    client.release_program_schedule_manual(&1);

    // Find schedule triggered event
    let event = find_event_by_topic(&env, symbol_short!("SchedTrg"));
    assert!(event.is_some(), "ScheduleTriggered event should be emitted");

    let (_, data) = event.unwrap();
    assert_event_has_version(&env, &data);
}

#[test]
fn test_multiple_schedule_triggers_emit_multiple_events() {
    let env = Env::default();
    let (client, _admin, _token, _token_admin) = setup_program(&env, 100_000);

    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);
    let release_timestamp = 1000;

    // Create two schedules
    client.create_program_release_schedule(&30_000, &release_timestamp, &r1);
    client.create_program_release_schedule(&40_000, &release_timestamp, &r2);

    // Advance time and trigger
    env.ledger().set_timestamp(release_timestamp + 1);
    client.trigger_program_releases();

    // Count schedule triggered events
    let events = env.events().all();
    let mut schedule_trigger_count = 0;
    for i in 0..events.len() {
        let event = events.get(i).unwrap();
        let topics = event.0;
        if topics.len() > 0 {
            let first_topic = topics.get(0).unwrap();
            if let Ok(sym) = Symbol::try_from_val(&env, &first_topic) {
                if sym == symbol_short!("SchedTrg") {
                    schedule_trigger_count += 1;
                }
            }
        }
    }

    assert_eq!(schedule_trigger_count, 2, "Two ScheduleTriggered events should be emitted");
}

#[test]
fn test_aggregate_stats_includes_scheduled_count() {
    let env = Env::default();
    let (client, _admin, _token, _token_admin) = setup_program(&env, 100_000);

    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);

    // Create two schedules
    client.create_program_release_schedule(&30_000, &1000, &r1);
    client.create_program_release_schedule(&40_000, &2000, &r2);

    // Do a payout to trigger aggregate stats
    let r3 = Address::generate(&env);
    client.single_payout(&r3, &5_000);

    // Find aggregate stats event
    let event = find_event_by_topic(&env, symbol_short!("AggStats"));
    assert!(event.is_some());

    let (_, data) = event.unwrap();
    let data_map: Map<Symbol, Val> = Map::try_from_val(&env, &data).unwrap();
    
    let scheduled_count = u32::try_from_val(&env, &data_map.get(Symbol::new(&env, "scheduled_count")).unwrap()).unwrap();
    assert_eq!(scheduled_count, 2, "Should show 2 pending schedules");
}

#[test]
fn test_aggregate_stats_after_schedule_release() {
    let env = Env::default();
    let (client, _admin, _token, _token_admin) = setup_program(&env, 100_000);

    let recipient = Address::generate(&env);

    // Create schedule
    client.create_program_release_schedule(&30_000, &1000, &recipient);

    // Release it
    env.ledger().set_timestamp(1001);
    client.trigger_program_releases();

    // Find aggregate stats event (emitted after trigger)
    let events = env.events().all();
    let mut found_aggregate = false;
    
    for i in 0..events.len() {
        let event = events.get(i).unwrap();
        let topics = event.0;
        if topics.len() > 0 {
            let first_topic = topics.get(0).unwrap();
            if let Ok(sym) = Symbol::try_from_val(&env, &first_topic) {
                if sym == symbol_short!("AggStats") {
                    found_aggregate = true;
                    let data_map: Map<Symbol, Val> = Map::try_from_val(&env, &event.1).unwrap();
                    let scheduled_count = u32::try_from_val(&env, &data_map.get(Symbol::new(&env, "scheduled_count")).unwrap()).unwrap();
                    assert_eq!(scheduled_count, 0, "Should show 0 pending schedules after release");
                }
            }
        }
    }

    assert!(found_aggregate, "AggregateStats event should be emitted after schedule trigger");
}

#[test]
fn test_event_payload_compactness() {
    let env = Env::default();
    let (client, _admin, _token, _token_admin) = setup_program(&env, 100_000);

    let recipient = Address::generate(&env);
    client.single_payout(&recipient, &30_000);

    // Verify all events have compact payloads (only necessary fields)
    let events = env.events().all();
    for i in 0..events.len() {
        let event = events.get(i).unwrap();
        let data = event.1;
        
        if let Ok(data_map) = Map::<Symbol, Val>::try_from_val(&env, &data) {
            // All event payloads should be maps with version field
            assert!(data_map.contains_key(Symbol::new(&env, "version")));
            
            // Verify field count is reasonable (not bloated)
            assert!(data_map.len() <= 10, "Event payload should be compact");
        }
    }
}

#[test]
fn test_all_analytics_events_have_program_id() {
    let env = Env::default();
    let (client, _admin, _token, _token_admin) = setup_program(&env, 100_000);

    let recipient = Address::generate(&env);
    
    // Create schedule
    client.create_program_release_schedule(&30_000, &1000, &recipient);
    
    // Do payout
    client.single_payout(&recipient, &15_000);
    
    // Trigger schedule
    env.ledger().set_timestamp(1001);
    client.trigger_program_releases();

    // Check all analytics events have program_id
    let events = env.events().all();
    let analytics_topics = vec![
        &env,
        symbol_short!("AggStats"),
        symbol_short!("LrgPay"),
        symbol_short!("SchedTrg"),
    ];

    for i in 0..events.len() {
        let event = events.get(i).unwrap();
        let topics = event.0;
        if topics.len() > 0 {
            let first_topic = topics.get(0).unwrap();
            if let Ok(sym) = Symbol::try_from_val(&env, &first_topic) {
                for j in 0..analytics_topics.len() {
                    if sym == analytics_topics.get(j).unwrap() {
                        let data_map: Map<Symbol, Val> = Map::try_from_val(&env, &event.1).unwrap();
                        assert!(
                            data_map.contains_key(Symbol::new(&env, "program_id")),
                            "Analytics event should contain program_id"
                        );
                    }
                }
            }
        }
    }
}

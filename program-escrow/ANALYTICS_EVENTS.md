# Program Escrow Analytics Events Enhancement

## Overview
Enhanced analytics events emitted by the program escrow contract for better observability and monitoring.

## New Event Types

### 1. AggregateStatsEvent (`AggStats`)
Emitted after payouts and schedule releases to provide comprehensive program statistics.

**Fields:**
- `version`: u32 (always 2)
- `program_id`: String
- `total_funds`: i128 - Total funds ever locked in the program
- `remaining_balance`: i128 - Current available balance
- `total_paid_out`: i128 - Total amount paid out (total_funds - remaining_balance)
- `payout_count`: u32 - Number of payouts completed
- `scheduled_count`: u32 - Number of pending (unreleased) schedules

**When Emitted:**
- After `single_payout()`
- After `batch_payout()`
- After `trigger_program_releases()` (if any releases occurred)

**Use Cases:**
- Real-time program health monitoring
- Dashboard analytics
- Alerting on low balances
- Tracking payout velocity

### 2. LargePayoutEvent (`LrgPay`)
Emitted when a single payout exceeds 10% of total program funds.

**Fields:**
- `version`: u32 (always 2)
- `program_id`: String
- `recipient`: Address
- `amount`: i128 - Payout amount
- `threshold`: i128 - The 10% threshold that was exceeded

**When Emitted:**
- During `single_payout()` if amount >= 10% of total_funds
- During `batch_payout()` for each individual payout >= 10% of total_funds

**Use Cases:**
- Fraud detection
- Unusual activity monitoring
- Compliance tracking
- Security alerts

### 3. ScheduleTriggeredEvent (`SchedTrg`)
Emitted when a release schedule is executed (either automatically or manually).

**Fields:**
- `version`: u32 (always 2)
- `program_id`: String
- `schedule_id`: u64
- `recipient`: Address
- `amount`: i128
- `trigger_type`: ReleaseType (Manual | Automatic | Oracle)

**When Emitted:**
- During `trigger_program_releases()` for each released schedule (Automatic)
- During `release_program_schedule_manual()` (Manual)
- During `release_prog_schedule_automatic()` (Automatic)

**Use Cases:**
- Schedule execution tracking
- Audit trail for releases
- Automated vs manual release analytics
- Recipient notification triggers

## Implementation Details

### Event Payload Design
All events follow the v2 schema with:
- Compact payloads (only essential fields)
- Consistent `version` field for forward compatibility
- `program_id` for multi-tenant filtering
- Expressive but minimal data

### Threshold Logic
Large payout threshold is calculated as:
```rust
threshold = program_data.total_funds / 10  // 10%
if amount >= threshold {
    emit LargePayoutEvent
}
```

### Aggregate Stats Calculation
Scheduled count is computed by iterating pending schedules:
```rust
for schedule in schedules {
    if !schedule.released {
        scheduled_count += 1
    }
}
```

## Testing

Comprehensive test suite in `src/test_analytics_events.rs`:

1. **test_aggregate_stats_event_on_single_payout** - Verifies aggregate stats after single payout
2. **test_aggregate_stats_event_on_batch_payout** - Verifies aggregate stats after batch payout
3. **test_large_payout_event_emitted_above_threshold** - Tests large payout detection (15% of funds)
4. **test_large_payout_event_not_emitted_below_threshold** - Tests threshold boundary (5% of funds)
5. **test_large_payout_event_in_batch** - Tests large payout detection in batch operations
6. **test_schedule_triggered_event_automatic** - Tests automatic schedule trigger events
7. **test_schedule_triggered_event_manual** - Tests manual schedule trigger events
8. **test_multiple_schedule_triggers_emit_multiple_events** - Tests multiple schedule releases
9. **test_aggregate_stats_includes_scheduled_count** - Verifies scheduled_count calculation
10. **test_aggregate_stats_after_schedule_release** - Verifies stats update after release
11. **test_event_payload_compactness** - Ensures payloads are not bloated
12. **test_all_analytics_events_have_program_id** - Verifies program_id in all events

## Security Considerations

1. **No Sensitive Data**: Events contain only necessary operational data
2. **Threshold-Based Alerts**: Large payout events enable fraud detection
3. **Audit Trail**: Schedule triggered events provide complete execution history
4. **Version Compatibility**: v2 schema ensures forward compatibility

## Performance Impact

- **Minimal**: Event emission is O(1) for payouts
- **Scheduled Count**: O(n) where n = number of schedules (typically small)
- **No Storage Overhead**: Events are emitted, not stored on-chain

## Integration Guide

### Indexer Integration
```typescript
// Listen for aggregate stats
contract.on('AggStats', (event) => {
  const { program_id, total_paid_out, remaining_balance } = event;
  updateDashboard(program_id, { total_paid_out, remaining_balance });
});

// Monitor large payouts
contract.on('LrgPay', (event) => {
  const { program_id, recipient, amount, threshold } = event;
  alertSecurityTeam(program_id, recipient, amount, threshold);
});

// Track schedule executions
contract.on('SchedTrg', (event) => {
  const { schedule_id, recipient, trigger_type } = event;
  notifyRecipient(recipient, schedule_id, trigger_type);
});
```

### SubQuery Integration
```graphql
query ProgramAnalytics($programId: String!) {
  aggregateStatsEvents(
    filter: { programId: { equalTo: $programId } }
    orderBy: TIMESTAMP_DESC
    first: 1
  ) {
    nodes {
      totalFunds
      remainingBalance
      totalPaidOut
      payoutCount
      scheduledCount
    }
  }
  
  largePayoutEvents(
    filter: { programId: { equalTo: $programId } }
    orderBy: TIMESTAMP_DESC
  ) {
    nodes {
      recipient
      amount
      threshold
      timestamp
    }
  }
}
```

## Backward Compatibility

All new events are additive and do not affect existing functionality:
- Existing events (`PrgInit`, `FndsLock`, `BatchPay`, `Payout`) unchanged
- New events use v2 schema consistent with existing events
- No breaking changes to contract interface

## Future Enhancements

Potential additions for future versions:
1. **Velocity Metrics**: Payout rate over time windows
2. **Recipient Analytics**: Per-recipient payout summaries
3. **Budget Alerts**: Configurable threshold alerts
4. **Gas Optimization**: Batch event emission for multiple schedules

## Documentation Updates

Event schema documentation should be updated in `EVENT_SCHEMA.md`:
- Add AggregateStatsEvent section
- Add LargePayoutEvent section
- Add ScheduleTriggeredEvent section
- Update event topic reference table
- Add payload field reference

## Changelog

### Added
- `AggregateStatsEvent` for comprehensive program statistics
- `LargePayoutEvent` for fraud detection and monitoring
- `ScheduleTriggeredEvent` for schedule execution tracking
- Helper function `emit_aggregate_stats()` for stats emission
- Helper function `check_and_emit_large_payout()` for threshold detection
- Comprehensive test suite with 12 test cases

### Modified
- `batch_payout()` - Added large payout detection and aggregate stats emission
- `single_payout()` - Added large payout detection and aggregate stats emission
- `trigger_program_releases()` - Added schedule triggered events and aggregate stats
- `release_program_schedule_manual()` - Added schedule triggered event
- `release_prog_schedule_automatic()` - Added schedule triggered event

### Constants Added
- `AGGREGATE_STATS: Symbol = symbol_short!("AggStats")`
- `LARGE_PAYOUT: Symbol = symbol_short!("LrgPay")`
- `SCHEDULE_TRIGGERED: Symbol = symbol_short!("SchedTrg")`

## Test Coverage

Target: 95%+ coverage for analytics event emission paths

Coverage includes:
- ✅ Single payout analytics
- ✅ Batch payout analytics
- ✅ Large payout threshold detection (above and below)
- ✅ Schedule trigger events (automatic and manual)
- ✅ Aggregate stats calculation
- ✅ Event payload structure validation
- ✅ Multi-schedule scenarios
- ✅ Edge cases (zero schedules, multiple large payouts)

## Security Notes

1. **Threshold Calculation**: Uses integer division, no floating point
2. **No Reentrancy**: Events emitted within reentrancy-protected functions
3. **Data Integrity**: All event data sourced from validated contract state
4. **Access Control**: Event emission follows existing authorization patterns

## Deployment Checklist

- [x] Implement new event structures
- [x] Add event emission logic to payout functions
- [x] Add event emission logic to schedule functions
- [x] Create comprehensive test suite
- [x] Document event schemas
- [ ] Update EVENT_SCHEMA.md
- [ ] Run full test suite
- [ ] Security audit of event emission paths
- [ ] Deploy to testnet
- [ ] Verify events in testnet explorer
- [ ] Update SDK with new event types
- [ ] Deploy to mainnet

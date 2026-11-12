# Reddart Write-Ahead Log (WAL) Transaction Semantics & Durability Specification

## 1. Executive Summary

Reddart video editing workloads require durable, crash-resilient project persistence. Unsaved edits cannot be lost due to power interruption, kernel panic, or application crash. Reddart implements an industrial-grade Write-Ahead Log (WAL) engine rooted in ARIES database transaction principles:

1. **Write-Ahead Invariant**: Every project mutation is serialized, checksummed, and fsynced to physical disk storage before the transaction is acknowledged as committed to the user.
2. **Crash-Safe Atomicity**: Incomplete or interrupted writes (torn tails) are truncated to the last known-good record offset.
3. **Sequential Integrity**: Gaps, regressions, and non-monotonic sequence numbers halt replay to preserve valid project state.
4. **Isolated Mutation Replay**: Failed mutations are isolated and not counted toward replayed edits.
5. **Rollback on Append Failure**: If storage I/O fails during WAL append, the in-memory mutation is immediately reverted via the inverse command, leaving in-memory state clean and preventing desynchronization.

---

## 2. Transaction Architecture & Lifecycle

### 2.1 The Mutation Pipeline

```
[ User Interaction / Edit Command ]
                 │
                 ▼
     [ 1. Dry Run / In-Memory Apply ]
       - Validates edit against timeline rules
       - Checks boundaries, tracks, clips
                 │
       (Success) │ (Fail -> CommandError, Log Unmodified)
                 ▼
     [ 2. Durable WAL Append & Fsync ]
       - Increments monotonic sequence `seq += 1`
       - Generates UUID v4 transaction ID
       - Precomputes 32-bit CRC32 checksum
       - Appends newline-delimited record to `.reddart.wal`
       - Executes `file.flush()` & `file.sync_all()`
                 │
       ┌─────────┴─────────┐
       │ (WAL Write Fails) │ (WAL Write Succeeded)
       ▼                   ▼
[ 2b. In-Memory Rollback ] [ 3. Commit Acknowledged ]
  - Invokes inverse cmd      - Pushes cmd to Undo Stack
  - Reverts project state    - Clears Redo Stack
  - Returns JournalError     - Updates HUD & UI
  - State remains clean      - Project marked touched
```

### 2.2 Reversible Undo/Redo with Durable WAL Integration

When a user executes `Ctrl+Z` (Undo) or `Ctrl+Y` (Redo):
1. **Undo**:
   - The command is popped from `undo_stack`.
   - The computed `inverse()` command is applied in-memory.
   - Corresponding inverse mutations are appended to the WAL and fsynced.
   - If WAL append fails, the original command is reapplied in-memory, the command is returned to `undo_stack`, and `CommandError::JournalError` is returned.
   - On success, the command is pushed to `redo_stack`.
2. **Redo**:
   - The command is popped from `redo_stack`.
   - The command is applied in-memory.
   - Corresponding forward mutations are appended to the WAL and fsynced.
   - If WAL append fails, the inverse command is reapplied in-memory, the command is returned to `redo_stack`, and `CommandError::JournalError` is returned.
   - On success, the command is pushed to `undo_stack`.

---

## 3. On-Disk WAL Record Format & Integrity

WAL entries are stored in a companion file adjacent to the project file (`<project_name>.reddart.wal`) using newline-delimited JSON:

```json
{
  "seq": 42,
  "tx_id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
  "timestamp_ms": 1773278900000,
  "mutation": {
    "SplitClip": {
      "track_id": 1,
      "clip_id": "8c59b667-27b9-4a0b-8d4e-1f7c320d7861",
      "split_time_us": 4000000,
      "new_clip": { ... }
    }
  },
  "checksum": 3284091823
}
```

### 3.1 CRC32 Checksum Validation
The checksum is calculated using the IEEE 802.3 polynomial (`0xEDB88320`) over the serialized tuple `(seq, tx_id, timestamp_ms, mutation)`. Any bit-flip, byte truncation, or sector corruption causes an immediate checksum mismatch.

---

## 4. Crash Recovery & Replay Semantics

When loading a project (`Project::load_file_with_recovery`):
1. The base project snapshot (`.reddart`) is deserialized.
2. If `.reddart.wal` exists, `ProjectJournal::replay_into(&mut project)` is executed.
3. For each line in the WAL:
   - **Checksum Check**: If CRC32 does not match, the record is flagged as corrupt or torn. Replay halts, and the WAL is safely truncated at `last_valid_offset`.
   - **Sequence Continuity Check**: If `entry.seq != expected_seq`, a sequence gap or regression has occurred. Replay halts, and the WAL is truncated at `last_valid_offset` to preserve the contiguous valid prefix.
   - **Mutation Application**: `mutation.apply(&mut project)` is executed.
     - If `Ok(())`: `valid_records_replayed` is incremented, and `last_valid_offset` advances.
     - If `Err(err)`: `failed_mutations` is incremented. The invalid mutation is NOT counted as replayed, and project state remains untainted.
4. **Torn Tail Truncation**: Any truncated bytes are synced via `file.sync_all()`, restoring the WAL to a fully consistent state. Subsequent edits append seamlessly starting at `last_seq + 1`.

---

## 5. Checkpointing & Journal Compaction

Checkpoints consolidate accumulated WAL transactions into the base `.reddart` snapshot file:

1. **Step 1: Atomic Project Save (`project.save_atomic`)**:
   - Serializes complete project state to a temporary file (`.reddart.tmp.<uuid>`).
   - Flushes and syncs temporary file to disk.
   - Performs atomic file rename (`fs::rename`) replacing `<project_name>.reddart`.
2. **Step 2: Journal Compaction**:
   - Truncates `<project_name>.reddart.wal`.
   - Appends a `JournalMutation::Checkpoint { sequence, snapshot_time_ms }` sentinel.
   - Flushes and syncs WAL file.

### Crash Resilience During Checkpointing:
- **Crash before Step 1 finishes**: The original `.reddart` file is untouched, and all uncompacted WAL mutations remain intact in `.reddart.wal`. Recovery replays all mutations into the old snapshot.
- **Crash between Step 1 and Step 2**: The new `.reddart` file is already durable. Replaying WAL mutations is either redundant or truncated, resulting in zero data loss.
- **Crash during Step 2**: Any torn sentinel in `.reddart.wal` is truncated to 0 bytes; the new `.reddart` file has the authoritative state. Zero data loss is guaranteed in all crash vectors.

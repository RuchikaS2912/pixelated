# Tests

Tests live inside each crate (standard Cargo layout):

- `core/tests/schedule_test.rs` — scheduling correctness: hourly/daily/
  weekly/once, disabled, sleep coalescing, DST spring-forward and
  fall-back, timezone pinning, start/end dates, duplicate prevention
- `core/src/{store,config,util,recurrence}.rs` — unit tests incl.
  persistence corruption recovery
- `daemon/src/{queue,ipc,spawn,runner}.rs` — queue semantics, IPC
  round-trip + server, renderer pool, scheduler tick simulation
- `renderer/src/{character,anim}.rs` — character loading/validation,
  walk-plan math
- `cli/tests/cli.rs` — end-to-end CLI suite against the real binary:
  every command, plus full daemon start → reminder fires (headless,
  WALKING_REMINDER_NO_RENDER=1) → stop cycles

Run everything headless:

    WALKING_REMINDER_NO_RENDER=1 cargo test --workspace

On macOS you can additionally verify the real overlay visually:

    ./target/release/walking-reminder test
    # or render one frame to a file:
    walking-reminder __render --message "hi" --screenshot frame.png

# `retrieval.json` has the same lost-update shape the pin store just fixed

**2026-08-18**, spotted while building `trust-store-serialises-read-modify-write-so-no-bless-is-lost` (spec `trust-store-hardening`).

`retrieval::apply_settings_request_in` (`crates/werust-core/src/retrieval.rs`, ~line 761) is a read-modify-write with no mutual exclusion: `RetrievalSettings::load_from(dir)` then `settings.save_to(dir)`, so two windows changing two different settings at the same moment lose one change exactly as two blesses used to. Its write is also a bare `fs::write` (no temp-then-rename), so an interrupted save can truncate it, where `pins.json`'s cannot.

NOT fixed here on purpose: that store's mutation path is spec `settings-mutations-require-user-intent`'s territory and widening this task's blast radius would collide with it. The mechanism is now sitting next door if it is wanted — `pins::TrustedNamePins::update_in` plus the `WriteLock` beside it, `docs/spikes/trust-store-serialises-read-modify-write-so-no-bless-is-lost/DECISIONS.md`. Note the stakes are lower than the trust store's: losing a retrieval-backend change is a wrong setting the user can see and re-apply, not a warning that silently never fires.

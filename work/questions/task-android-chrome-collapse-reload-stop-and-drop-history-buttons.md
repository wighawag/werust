<!-- dorfl-sidecar: item=task:android-chrome-collapse-reload-stop-and-drop-history-buttons type=task slug=android-chrome-collapse-reload-stop-and-drop-history-buttons allAnswered=false -->

## Q1

**'task:android-chrome-collapse-reload-stop-and-drop-history-buttons' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The new test the_mobile_presentation_guard_field_lists_are_not_registered_here cannot fail, so the MIGRATE/CONTRACT sequencing it claims to protect is unguarded. It does scan(guard) and then asserts !code.contains(field), but scan strips string literals out of the code view and collects them separately, and a field can only ever appear in mobile_chrome_presentation_shape.rs as a string literal (a DERIVED_FIELDS entry). Proof: the already-registered loadProgressVisible occurs at lines 117, 334 and 362 of that guard, all three inside literals, so the code view contains it nowhere. Registering loadSpinnerVisible tomorrow would therefore leave this assertion GREEN. Fix by checking the literals half (the same scan already returns it), or drop the assertion instead of documenting it as protection. (crates/werust-android/rust/tests/collapsed_control_and_dropped_history_buttons_shape.rs the_mobile_presentation_guard_field_lists_are_not_registered_here; scan() pushes literal contents to `literals` and never to `code`; DECISIONS.md section 7 says asserting the absence is what stops a well-meaning later change from registering it early, and the spike README says the suite was mutation-checked (its five listed mutations do not include this one))
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):

## Q2

**'task:android-chrome-collapse-reload-stop-and-drop-history-buttons' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The new test the_mobile_presentation_guard_field_lists_are_not_registered_here cannot fail, so the MIGRATE/CONTRACT sequencing it claims to protect is unguarded. It does scan(guard) and then asserts !code.contains(field), but scan strips string literals out of the code view and collects them separately, and a field can only ever appear in mobile_chrome_presentation_shape.rs as a string literal (a DERIVED_FIELDS entry). Proof: the already-registered loadProgressVisible occurs at lines 117, 334 and 362 of that guard, all three inside literals, so the code view contains it nowhere. Registering loadSpinnerVisible tomorrow would therefore leave this assertion GREEN. Fix by checking the literals half (the same scan already returns it), or drop the assertion instead of documenting it as protection. (crates/werust-android/rust/tests/collapsed_control_and_dropped_history_buttons_shape.rs the_mobile_presentation_guard_field_lists_are_not_registered_here; scan() pushes literal contents to `literals` and never to `code`; DECISIONS.md section 7 says asserting the absence is what stops a well-meaning later change from registering it early, and the spike README says the suite was mutation-checked (its five listed mutations do not include this one))
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q2 fields: id=q2 kind=stuck -->

**Your answer** (write below this line):

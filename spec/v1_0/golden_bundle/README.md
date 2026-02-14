# Golden Bundle

This directory contains canonical golden bundle artifacts.

Invariant:
For a given distribution identity, the golden bundle bytes are stable and must match exactly.

Generation and verification are performed by:
- tools/gen_golden_bundle_v1.sh
- tools/verify_golden_bundle_v1.sh
- tools/stop_condition_clean_checkout_golden_v1.sh

The stop condition script must pass on a clean checkout.

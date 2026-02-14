ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

TMP="$(mktemp -d)"
REPO="$TMP/repo"
git clone . "$REPO" >/dev/null 2>&1

cd "$REPO" && \
bash tools/sync_spec_tmp_from_tests.sh && \
cargo test -q -- --test-threads=1 && \
bash tools/gen_golden_bundle_v1.sh && \
bash tools/verify_golden_bundle_v1.sh && \
printf "OK stop condition clean checkout + golden verified\n" || \
false

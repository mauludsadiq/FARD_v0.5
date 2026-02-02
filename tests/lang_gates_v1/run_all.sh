ROOT="tests/lang_gates_v1"

sh "$ROOT/run.sh" || exit 1

find "$ROOT" -type f -name "run_g*.sh" -print | sort | while read -r s; do
  sh "$s" || exit 1
done

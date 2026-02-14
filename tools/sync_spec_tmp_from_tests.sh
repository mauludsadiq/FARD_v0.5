ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

mkdir -p spec/tmp

extract_one() {
  local want="$1"
  local test_rs="$2"

  WANT="$want" PATH_OUT="$want" perl -0777 -ne '
    my $want = $ENV{"WANT"};
    my $path = $ENV{"PATH_OUT"};
    my $re = quotemeta($want);

    if ($_ =~ m/write_prog\(\s*"$re"\s*,\s*(r#"+.*?"#+|".*?")\s*\)/s) {
      my $lit = $1;

      my $src = "";
      if ($lit =~ m/^r(#+)\"(.*)\"\1$/s) { $src = $2; }
      elsif ($lit =~ m/^\"(.*)\"$/s) {
        $src = $1;
        $src =~ s/\\n/\n/g;
        $src =~ s/\\r/\r/g;
        $src =~ s/\\t/\t/g;
        $src =~ s/\\"/"/g;
        $src =~ s/\\\\/\\/g;
      } else {
        print STDERR "UNSUPPORTED_LITERAL\n";
        exit 2;
      }

      open my $fh, ">", $path or die "WRITE_FAIL";
      print $fh $src;
      close $fh;
      print "WROTE $path\n";
      exit 0;
    }

    print STDERR "NOT_FOUND\n";
    exit 3;
  ' "$test_rs"
}

paths="$(rg -n --hidden --no-ignore-vcs 'write_prog\(\s*"spec/tmp/[^"]+"' tests -S \
  | perl -ne 'if (m/write_prog\(\s*"([^"]+)"/) { print "$1\n"; }' \
  | sort -u)"

missing=0

printf "%s\n" "$paths" | while IFS= read -r p; do
  [ -n "$p" ] || continue

  if [ -f "$p" ]; then
    continue
  fi

  test_rs="$(rg -n --hidden --no-ignore-vcs "write_prog\\(\\s*\"$p\"" tests -S | head -n 1 | cut -d: -f1)"
  if [ -z "$test_rs" ]; then
    printf "NO_TEST_SOURCE_FOR %s\n" "$p"
    missing=1
    continue
  fi

  if extract_one "$p" "$test_rs"; then
    :
  else
    printf "FAILED_EXTRACT %s\n" "$p"
    missing=1
  fi
done

if [ "$missing" = "0" ]; then
  printf "OK spec/tmp fixtures present\n"
else
  printf "FAIL spec/tmp fixtures missing\n"
  false
fi

#!/bin/sh
# SPDX-License-Identifier: MIT
#
# Builds and runs the examples as their README files say and checks every answer against the
# README. It runs from the root of the repository:
#
#   sh examples/check.sh [bookshelf] [worker]
#
# With no name it checks every example. Each command below is a command of the README of its
# example, compared with the README after every line that ends in a backslash is joined to the
# next, and the check stops where the README no longer holds a command or an answer. A server runs
# in the background on the port its README names, with its output in target/examples/. The check
# refuses a port that answers already and stops every server it started, with the processes the
# server started. The bookshelf needs cargo and curl, the worker npm, Bun and curl.

set -eu

root=$(pwd)
logs=$root/target/examples
servers=
export DO_NOT_TRACK=1

fail() {
  echo "check: $*" >&2
  exit 1
}

# Stops every server the check started with its process group, by the interrupt of Ctrl+C, so a
# server cleans up as it does in a terminal, and kills a group that still runs after 10 seconds.
stop() {
  for group in $servers; do
    kill -INT "-$group" 2>/dev/null || true
    tries=0
    while kill -0 "-$group" 2>/dev/null && [ "$tries" -lt 100 ]; do
      tries=$((tries + 1))
      sleep 0.1
    done
    kill -KILL "-$group" 2>/dev/null || true
    wait "$group" 2>/dev/null || true
  done
  servers=
}
trap stop EXIT
trap 'exit 1' HUP INT TERM

# Prints the file $1, or stdin for -, with every line that ends in a backslash joined to the next.
joined() {
  awk '{
    line = $0
    if (open) sub(/^ +/, "", line)
    if (sub(/ *\\$/, "", line)) { held = held line " "; open = 1; next }
    print held line
    held = ""
    open = 0
  }' "$1"
}

# Fails unless the README $1 holds the line $2.
holds() {
  joined "$1" | grep -qxF -e "$2" || fail "$1 does not hold: $2"
}

# Prints the command on stdin and fails unless the README $1 holds it.
command_of() {
  text=$(cat)
  holds "$1" "$(printf '%s\n' "$text" | joined -)"
  printf '%s\n' "$text"
}

# Runs the command on stdin, a command of the README $1.
run() {
  cmd=$(command_of "$1")
  echo "+ $cmd"
  eval "$cmd" || fail "the command failed: $cmd"
}

# Starts the server on stdin, a command of the README $1, and waits until the port $2 answers.
serve() {
  cmd=$(command_of "$1")
  if curl -s -o /dev/null "localhost:$2"; then fail "port $2 answers already"; fi
  mkdir -p "$logs"
  log=$logs/port-$2.log
  echo "+ $cmd &"
  set -m
  eval "$cmd" > "$log" 2>&1 &
  server=$!
  set +m
  servers="$servers $server"
  tries=0
  until curl -s -o /dev/null "localhost:$2"; do
    kill -0 "$server" 2>/dev/null || fail "the server ended, see $log"
    tries=$((tries + 1))
    [ "$tries" -le 6000 ] || fail "nothing answers on port $2 after 10 minutes, see $log"
    sleep 0.1
  done
}

# Runs the request on stdin, a command of the README $1, and checks its answer against the README.
ask() {
  cmd=$(command_of "$1")
  echo "+ $cmd"
  answer=$(eval "$cmd") || fail "the request failed: $cmd"
  echo "$answer"
  holds "$1" "$answer"
}

# Where the workspace renamed or added a crate, cargo refuses the lock of the bookshelf under
# --locked. Its lock is the lock of the workspace pruned to the example, which these two commands
# write again from the root of the repository:
#
#   cp Cargo.lock examples/bookshelf/Cargo.lock
#   cargo metadata --offline --format-version 1 \
#     --manifest-path examples/bookshelf/Cargo.toml > /dev/null
bookshelf() {
  readme=$root/examples/bookshelf/README.md
  run "$readme" <<'EOF'
cargo build --locked -p aosabi-cli --bin cargo-aosabi
EOF
  run "$readme" <<'EOF'
target/debug/cargo-aosabi build --release --locked --manifest-path examples/bookshelf/Cargo.toml
EOF
  serve "$readme" 4000 <<'EOF'
PORT=4000 examples/bookshelf/target/aosabi/dist/bookshelf-server
EOF
  ask "$readme" <<'EOF'
curl -s localhost:4000/graphql -H 'content-type: application/json' \
  -d '{"query":"{ books(genre: SCIENCE) { title author year } }"}'
EOF
  ask "$readme" <<'EOF'
curl -s localhost:4000/health
EOF
  stop
}

worker() {
  readme=$root/examples/worker/README.md
  cd "$root/examples/worker"
  run "$readme" <<'EOF'
npm install --install-links --omit=optional --no-audit
EOF
  run "$readme" <<'EOF'
npx aosabi build
EOF
  serve "$readme" 4000 <<'EOF'
PORT=4000 bun --preload aosabi/bun .aosabi/server.mjs
EOF
  ask "$readme" <<'EOF'
curl -s localhost:4000/graphql -H 'content-type: application/json' \
  -d '{"query":"{ hello(name: \"Bun\") }"}'
EOF
  stop
  serve "$readme" 8787 <<'EOF'
WRANGLER_SEND_METRICS=false npx wrangler@3.114.17 dev
EOF
  ask "$readme" <<'EOF'
curl -s localhost:8787/graphql -H 'content-type: application/json' \
  -d '{"query":"{ hello(name: \"Worker\") }"}'
EOF
  stop
  cd "$root"
}

[ -f examples/check.sh ] || fail "run it from the root of the repository"
[ "$#" -gt 0 ] || set -- bookshelf worker
for example in "$@"; do
  case $example in
    bookshelf | worker)
      echo "== $example"
      "$example"
      ;;
    *) fail "no example named $example" ;;
  esac
done
echo "check: every command and answer matches its README"

#!/usr/bin/env bash
# mint an HS256 token from the box's gate secret. usage: mint-jwt.sh core@<ip> [sub] [years]
set -euo pipefail
HOST="$1"; SUB="${2:-ci}"; YEARS="${3:-10}"
KEY=$(ssh "$HOST" 'sudo k3s kubectl -n forest-edge get secret caddy-jwt -o jsonpath="{.data.sign_key}" | base64 -d')
[ -n "$KEY" ] || { echo "no caddy-jwt secret on $HOST -- run make jwt first"; exit 1; }

b64url() { openssl base64 -A | tr '+/' '-_' | tr -d '='; }
now=$(date +%s); exp=$((now + YEARS*365*24*3600))
hdr='{"alg":"HS256","typ":"JWT"}'
pl=$(printf '{"sub":"%s","iss":"forest-gate","iat":%s,"nbf":%s,"exp":%s}' "$SUB" "$now" "$now" "$exp")
si="$(printf '%s' "$hdr" | b64url).$(printf '%s' "$pl" | b64url)"
keyhex=$(printf '%s' "$KEY" | openssl base64 -d -A | od -An -v -tx1 | tr -d ' \n')
sig=$(printf '%s' "$si" | openssl dgst -sha256 -mac HMAC -macopt hexkey:"$keyhex" -binary | b64url)
echo "$si.$sig"

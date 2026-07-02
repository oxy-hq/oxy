# Accessing Airhouse (dev / prod)

How to connect to **Airhouse** (a DuckLake lakehouse exposed over the Postgres
pgwire protocol) running in the `oxy-dev` / `oxy-prod` EKS clusters, query it with
`psql`, and tear down cleanly.

Airhouse isn't exposed publicly. You reach it by `kubectl port-forward` into the
cluster, then mint a **short-lived** pgwire credential from the control-plane
Admin API. Nothing here is permanent — tokens auto-expire (≤ 24h).

```
your laptop ──port-forward──▶ <ns>-cp        (Admin API, mint creds)
            ──port-forward──▶ <ns>-haproxy   (pgwire: psql connects here)
```

## dev vs prod

Same topology, deployed under different release names and AWS accounts. The
scripts take an `env` argument (`dev` | `prod`) and resolve everything below:

| | **dev** | **prod** |
| --- | --- | --- |
| AWS profile / account | `oxy-dev` / `575455576647` | `oxy-prod` / `664267706513` |
| kube context | `oxy-dev` | `oxy-prod` |
| namespace (= release prefix) | `airhouse-dev` | `airhouse` |
| control-plane service | `airhouse-dev-cp` | `airhouse-cp` |
| pgwire service | `airhouse-dev-haproxy` | `airhouse-haproxy` |
| admin secret (key `token`) | `airhouse-dev-admin` | `airhouse-admin` |
| default obs tenant | `oxy-dev-observability` | `oxy-observability` |
| local ports (CP / serving / analytics) | `19090 / 25445 / 25446` | `19190 / 25545 / 25546` |

> Note: prod's admin secret is `airhouse-admin` (**not** `airhouse-prod-admin`) —
> the prod release is just named `airhouse`. Local port ranges differ per env so
> dev and prod tunnels can run at once and a dev session can't hit a prod forward.

## Prerequisites

- **AWS CLI v2**, **kubectl**, **jq**, **psql**, **python3** (base64 decode)
- Membership in the target AWS account via SSO

## One-time setup

### 1. Configure the AWS SSO profile(s)

Run `aws configure sso`, or add to `~/.aws/config` (org config, not secrets — your
`sso_role_name` may differ):

```ini
[profile oxy-dev]
sso_start_url  = https://oxy-tech.awsapps.com/start
sso_region     = us-west-2
sso_account_id = 575455576647
sso_role_name  = AdministratorAccess
region         = us-west-2

[profile oxy-prod]
sso_start_url  = https://oxy-tech.awsapps.com/start
sso_region     = us-west-2
sso_account_id = 664267706513
sso_role_name  = AdministratorAccess
region         = us-west-2
```

### 2. Add the EKS clusters to your kubeconfig

```bash
aws eks update-kubeconfig --region us-west-2 --name oxy-dev  --profile oxy-dev  --alias oxy-dev
aws eks update-kubeconfig --region us-west-2 --name oxy-prod --profile oxy-prod --alias oxy-prod
```

## Quick start

```bash
# 0. refresh SSO for the env you want (re-run whenever kubectl starts failing)
aws sso login --profile oxy-dev          # or oxy-prod

# 1. start the tunnels (leave running in this terminal)
./port-forward.sh dev                    # or: ./port-forward.sh prod

# 2. in another terminal: mint a temp read-only user and psql in
./connect_airhouse.sh dev                # or: ./connect_airhouse.sh prod
```

`connect_airhouse.sh` defaults to the observability tenant
(`oxy-dev-observability` for dev, `oxy-observability` for prod) as a **reader**
for 1h, and **auto-revokes** the temp credential when you exit psql (see
[Cleanup](#cleanup)). Override positionally:
`./connect_airhouse.sh <env> <tenant> <role> <ttl_secs>`.

## Step by step (what the scripts do)

### 1. SSO login

```bash
aws sso login --profile oxy-dev     # oxy-prod for prod
```

Opens a browser; kubectl / `aws eks get-token` then mint cluster tokens
transparently. Sessions expire (hours) — re-run when kubectl errors with
`The SSO session ... has expired`.

### 2. Port-forward (control-plane + pgwire)

`./port-forward.sh <env>` runs (for dev):

```bash
kubectl --context oxy-dev -n airhouse-dev port-forward --address 127.0.0.1 svc/airhouse-dev-cp 19090:8080 &
kubectl --context oxy-dev -n airhouse-dev port-forward --address 127.0.0.1 svc/airhouse-dev-haproxy 25445:5445 25446:5446 &
```

⚠️ **Port choice matters.** A locally-running airhouse binds `8080`/`15445` on
IPv4; if you reuse those, kubectl can only grab the IPv6 side and your
`127.0.0.1` traffic silently hits the *local* instance. The ports above avoid it.

### 3. Retrieve the control-plane admin token

```bash
kubectl --context oxy-dev -n airhouse-dev get secret airhouse-dev-admin \
  -o jsonpath='{.data.token}' | python3 -c 'import sys,base64;print(base64.b64decode(sys.stdin.read()).decode())'
```

### 4. Mint a temporary user (two calls)

Create a service account capped at the role you need, then mint a token from it:

```bash
SA_BEARER=$(curl -s -X POST http://127.0.0.1:19090/admin/v1/service-accounts \
  -H "Authorization: Bearer $ADMIN_TOKEN" -H "Content-Type: application/json" \
  -d '{"name":"reader-obs","tenant_id":"oxy-dev-observability","max_role":"reader","max_ttl_secs":3600}' | jq -r .bearer)

curl -s -X POST http://127.0.0.1:19090/admin/v1/tenants/oxy-dev-observability/tokens \
  -H "Authorization: Bearer $SA_BEARER" -H "Content-Type: application/json" \
  -d '{"subject":"reader","role":"reader","ttl_secs":3600}' | jq '{username,password,expires_at}'
```

### 5. Connect with psql

```bash
psql "postgres://<username>:<password>@127.0.0.1:25446/oxy-dev-observability"
```

Use the analytics port (`25446` dev / `25546` prod) for read queries.
Observability data lives in tables prefixed `oxy_obs_`:

```sql
\dt oxy_obs_*
```

## Roles

`reader` (SELECT), `writer` (SELECT/INSERT/UPDATE/DELETE), `admin` (+ DDL).
Airhouse stores tables in DuckLake — **DDL like `CREATE TABLE` / `SET SORTED BY` /
`SET PARTITIONED BY` needs `admin`**; `reader` is read-only. Prefer `reader`, and
**never run DDL/writes against prod tenants** unless you mean it.

## Cleanup

`connect_airhouse.sh` **auto-revokes** the temporary service account it created
when you exit psql (an `EXIT` trap — it works because the script doesn't `exec`,
so the admin token is still in scope). Tokens also expire on their own (≤ TTL).

To keep the credential alive after the script exits (e.g. to use the URL from
another tool), run with `KEEP=1`:

```bash
KEEP=1 ./connect_airhouse.sh dev oxy-dev-observability reader 7200
```

The script then prints a **self-contained** revoke command (it re-reads the admin
token from the secret, so there's no stale variable to depend on).

Stop the tunnels with Ctrl-C.

## Troubleshooting

- **`The SSO session ... has expired` / kubectl hangs** → `aws sso login --profile oxy-<env>`.
- **Admin API returns 401** → you're probably hitting a *local* airhouse on a
  colliding port. Confirm the forward bound the expected `127.0.0.1` port
  (`lsof -nP -iTCP:19090 -sTCP:LISTEN`), and that you used this env's port range.
- **`error: lost connection to pod` / psql `connection closed` mid-query** → the
  DP pod was evicted/rescheduled (node consolidation). Re-run the port-forward;
  for long sessions wrap it in a reconnect loop:
  `while true; do kubectl ... port-forward ...; sleep 1; done`.
- **psql `database "..." does not exist`** → the dbname must equal the tenant the
  token was minted for. List tenants with `GET /admin/v1/tenants` (needs the
  admin token) to confirm the exact name in that env.

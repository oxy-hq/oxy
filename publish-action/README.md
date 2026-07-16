# oxy-hq/publish-action

Publish a built customer-app bundle to [Oxy](https://oxygen-hq.com) from GitHub
Actions via **trusted publishing** — no stored token.

The action mints a GitHub Actions OIDC token, exchanges it for a short-lived,
app-scoped Oxy credential, and uploads your bundle. The customer stores no secret;
Oxy verifies the OIDC token's claims against a publisher you register once.

## Usage

`oxy init-ci` generates a ready workflow. The publish job must have
`id-token: write`, and should be isolated (only download-artifact + this action),
because `id-token: write` is visible to every step in the job:

```yaml
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: npm ci && npm run build
      - uses: actions/upload-artifact@v4
        with: { name: oxy-bundle, path: dist }

  publish:
    needs: build
    runs-on: ubuntu-latest
    environment: oxy-publish          # attach required-reviewers to gate publishes
    permissions:
      id-token: write                 # the entire secret story
      contents: read
    steps:
      - uses: actions/download-artifact@v4
        with: { name: oxy-bundle, path: dist }
      - uses: oxy-hq/publish-action@v1
        with:
          app: northwind/dashboard    # org-slug/app-slug
          dir: dist
          promote: 'true'             # else publishes to the draft channel
```

## Inputs

| Input | Required | Default | Description |
| --- | --- | --- | --- |
| `app` | yes | — | The app, `org-slug/app-slug`. |
| `dir` | yes | `dist` | Built bundle directory (must contain `index.html`). |
| `target` | no | `https://app.oxygen-hq.com` | Oxy base URL. |
| `promote` | no | `false` | Promote to the published channel (else draft). |

## One-time setup

1. `oxy init-ci` in your repo (writes the workflow above).
2. Register this workflow as a **publisher** for the app (an Oxy operator, or the
   app owner): repo owner + **numeric** owner id + repo name + workflow path +
   environment. `oxy init-ci` prints the exact values.
3. The app's organization must turn on **partner app publishing** (Org Settings)
   if you are a partner publishing on their behalf.

## Source

This action lives in the Oxy monorepo at `publish-action/` and is mirrored to the
standalone `oxy-hq/publish-action` repo for `uses:` resolution.

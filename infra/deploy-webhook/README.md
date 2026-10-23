# Deploy webhook

Webhook listener for GitHub Actions to trigger deployments without requiring SSH access.

## How it works

1. GitHub Actions workflow completes successfully
2. Workflow sends a signed POST request to `https://getcmdr.com/hooks/deploy-website`
3. Caddy forwards the request to the local webhook listener
4. Webhook verifies the HMAC-SHA256 signature
5. If valid, runs `deploy-website.sh`

The deploy script builds the new Docker image **before** stopping the old container. If the build fails, the existing
site stays up.

## Files

- `hooks.json`: Webhook configuration (reads secret from env var)
- `deploy-website.sh`: The actual deployment script

## Logs

Deploy output is appended to `/var/log/cmdr/deploy-website.log` on the server.

To view recent deploy logs:

```bash
ssh hetzner "tail -50 /var/log/cmdr/deploy-website.log"
```

## Granting journalctl access to the deploy user

The `deploy-cmdr` user can't read systemd journal by default. To fix that without giving sudo, add it to the
`systemd-journal` group:

```bash
sudo usermod -aG systemd-journal deploy-cmdr
```

After that, `deploy-cmdr` can run `journalctl` to see service logs (read-only, no sudo needed).

## Cloudflare cache purge

After the new container is up, `deploy-website.sh` purges Cloudflare's cache for the getcmdr.com zone, since Cloudflare
keeps pages at the edge for up to an hour (`apps/website/DETAILS.md` § Edge caching). A deploy whose purge fails exits 1
with an `ERROR` line in the log. The token is a Cloudflare API token that can only purge this zone's cache. systemd
hands it to the listener through `EnvironmentFile=/etc/cmdr/deploy-webhook.env` (root-owned, mode `0640`, group
`deploy-cmdr`), which holds one line: `CLOUDFLARE_PURGE_TOKEN=...`. Its source of truth is `CMDR_CLOUDFLARE_PURGE_TOKEN`
in the sops secrets store.

## Security

The webhook uses HMAC-SHA256 signature verification. Only requests signed with the correct secret are accepted. The
secret is stored in:

- GitHub: Repository secret `DEPLOY_WEBHOOK_SECRET`
- Server: Environment variable loaded by systemd
